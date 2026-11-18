use application_core::AuditActor;
use async_trait::async_trait;
use erp_audit::AuditActorLogs;
use erp_audit::execution::persist_log;
use erp_core::AccountKind;
use erp_core::common::time::Instant;
use erp_supply::after_sales::{AfterSalesExt, SettlementEvidence, SettlementScan, SettlementScanState};
use erp_supply::entity::supplier_api::SupplierApiConnection;
use erp_supply::ports::connector::common::{Page, Snapshot};
use erp_supply::ports::connector::settlement::{SettlementEntry, Settlements};
use erp_supply::ports::supplier_api_gateway::SupplierApiGateway;
use erp_supply::repository::supplier_catalog;
use persistence_core::{NoTransaction, Transactional};

use super::SupplierAfterSalesProcess;
use crate::{Error, Result};

#[async_trait]
pub(super) trait SettlementStore: Send + Sync {
    async fn save_page(
        &self,
        scan: &mut SettlementScan,
        page: Page<Snapshot<SettlementEntry>>,
    ) -> Result<usize>;
    async fn save_scan(&self, scan: &mut SettlementScan) -> Result<()>;
}

pub(super) async fn collect(
    store: &dyn SettlementStore,
    source: Option<&dyn Settlements>,
    scan: &mut SettlementScan,
) -> Result<usize> {
    let Some(source) = source else {
        if scan.state != SettlementScanState::Unsupported {
            scan.state = SettlementScanState::Unsupported;
            scan.blocker = Some(
                "SUPPLIER_SETTLEMENT_UNSUPPORTED: 当前协议不支持自动结算来源采集，请人工对账；已接收证据保留"
                    .into(),
            );
            store.save_scan(scan).await?;
        }
        return Ok(0);
    };

    scan.resume(Instant::now())?;
    scan.next_run_at = Instant::from_unix_secs(Instant::now().unix_secs().saturating_add(300));
    store.save_scan(scan).await?;

    let page = source
        .entries(&scan.query)
        .await
        .map_err(|_| Error::BusinessLogicError("结算来源查询失败，原窗口与游标保留".into()))?;
    if page.items.len() > 50 {
        return Err(Error::BusinessLogicError("结算来源超过50条分页上限".into()));
    }
    store.save_page(scan, page).await
}

struct MongoSettlementStore<'a>(&'a SupplierAfterSalesProcess);
#[async_trait]
impl SettlementStore for MongoSettlementStore<'_> {
    async fn save_scan(&self, scan: &mut SettlementScan) -> Result<()> {
        self.0.db.api_settlement_scans().update(scan, &mut NoTransaction).await?;
        Ok(())
    }
    async fn save_page(
        &self,
        scan: &mut SettlementScan,
        page: Page<Snapshot<SettlementEntry>>,
    ) -> Result<usize> {
        let rows = page
            .items
            .into_iter()
            .map(|item| SettlementEvidence::new(scan, item))
            .collect::<std::result::Result<Vec<_>, _>>()?;

        let mut saved = scan.clone();
        saved.advance(page.next, Instant::now())?;
        let db = self.0.db.clone();
        let (checkpoint, count) = db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    db.api_settlement_scans().update(&mut saved, executor).await?;
                    let mut count = 0;
                    for row in &rows {
                        if let Some(old) =
                            db.api_settlement_evidence().find_by_id(&row.base.id, executor).await?
                        {
                            old.ensure_replay(row)?;
                        } else {
                            db.api_settlement_evidence().create(row, executor).await?;
                            count += 1;
                        }
                    }

                    let actor = AuditActor::new(
                        "system:supplier-after-sales".into(),
                        "供应商售后处理器".into(),
                        AccountKind::Admin,
                    );
                    let audit = actor.resource_log(
                        "supplier_settlement.receive",
                        "supplier_api_connection",
                        saved.base.id.clone(),
                    )?;
                    persist_log(&db, &audit, executor).await?;
                    Ok::<_, Error>((saved, count))
                })
            })
            .await?;

        *scan = checkpoint;
        Ok(count)
    }
}

impl SupplierAfterSalesProcess {
    /// 有界扫描连接并读取每个账号一页；不支持时记录明确状态。
    ///
    /// 本轮新接收的来源流水数，不是财务核销数。
    ///
    /// 连接分页查询失败；单连接异常不影响其他连接。
    pub async fn run_settlements_once(&self) -> Result<usize> {
        let mut cursor = self.cursor.lock().await;
        let connections = supplier_catalog::connection_page(
            &self.db,
            &cursor,
            &self.registry.catalog_connector_types(),
            &mut NoTransaction,
        )
        .await?;
        if connections.is_empty() {
            cursor.clear();
            return Ok(0);
        }

        let mut total = 0;
        for connection in connections {
            *cursor = connection.base.id.clone();
            match self.collect_connection(&connection).await {
                Ok(count) => total += count,
                Err(_) => {
                    tracing::warn!(connection_id = %connection.base.id, "结算来源采集受阻，原窗口及已收证据保留")
                },
            }
        }
        Ok(total)
    }

    async fn collect_connection(&self, connection: &SupplierApiConnection) -> Result<usize> {
        let connector = self
            .registry
            .after_sales_connector(connection)
            .map_err(|e| Error::BusinessLogicError(e.summary))?;
        let mut scan =
            match self.db.api_settlement_scans().find_by_id(&connection.base.id, &mut NoTransaction).await? {
                Some(scan) => scan,
                None => {
                    let created = i64::try_from(connection.base.created_at)
                        .map_err(|_| Error::ValidationError("连接时间无效".into()))?;
                    let scan = SettlementScan::new(
                        connection.base.id.clone(),
                        connection.supplier_id.to_string(),
                        Instant::from_unix_secs(created),
                        Instant::now(),
                    );
                    self.db.api_settlement_scans().create(&scan, &mut NoTransaction).await?;
                    scan
                },
            };
        if scan.supplier_id != connection.supplier_id.as_ref() {
            return Err(Error::ConflictError("结算检查点供应商归属变化".into()));
        }
        if scan.next_run_at > Instant::now() {
            return Ok(0);
        }

        let result = collect(&MongoSettlementStore(self), connector.settlements(), &mut scan).await;
        if result.is_err() {
            scan.state = SettlementScanState::Blocked;
            scan.blocker = Some("结算来源读取或保存未完成，请核对原窗口；未推进财务状态".into());
            let _ = self.db.api_settlement_scans().update(&mut scan, &mut NoTransaction).await;
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use erp_supply::ports::connector::common::{ConnectorResult, Scan};

    use super::*;
    struct Store {
        trace: Arc<Mutex<Vec<&'static str>>>,
        fail_checkpoint: bool,
        fail_page: bool,
        stored: Mutex<SettlementScan>,
    }
    #[async_trait]
    impl SettlementStore for Store {
        async fn save_scan(&self, scan: &mut SettlementScan) -> Result<()> {
            self.trace.lock().unwrap().push("checkpoint");
            if self.fail_checkpoint {
                return Err(Error::ConflictError("checkpoint failed".into()));
            }
            *self.stored.lock().unwrap() = scan.clone();
            Ok(())
        }
        async fn save_page(
            &self,
            scan: &mut SettlementScan,
            page: Page<Snapshot<SettlementEntry>>,
        ) -> Result<usize> {
            self.trace.lock().unwrap().push("page");
            if self.fail_page {
                return Err(Error::ConflictError("page failed".into()));
            }
            assert!(page.items.is_empty());
            scan.advance(page.next, Instant::now())?;
            *self.stored.lock().unwrap() = scan.clone();
            Ok(0)
        }
    }
    struct Source {
        trace: Arc<Mutex<Vec<&'static str>>>,
        queries: Mutex<Vec<Scan>>,
    }
    #[async_trait]
    impl Settlements for Source {
        async fn entries(&self, query: &Scan) -> ConnectorResult<Page<Snapshot<SettlementEntry>>> {
            self.trace.lock().unwrap().push("read");
            self.queries.lock().unwrap().push(query.clone());
            Ok(Page { items: vec![], next: if query.cursor.is_none() { Some("next".into()) } else { None } })
        }
    }
    fn fixture() -> (Store, Source, SettlementScan) {
        let scan = SettlementScan::new(
            "connection".into(),
            "supplier".into(),
            Instant::from_unix_secs(100),
            Instant::from_unix_secs(200),
        );
        let trace = Arc::new(Mutex::new(vec![]));
        (
            Store {
                trace: trace.clone(),
                fail_checkpoint: false,
                fail_page: false,
                stored: Mutex::new(scan.clone()),
            },
            Source { trace, queries: Mutex::new(vec![]) },
            scan,
        )
    }
    #[tokio::test]
    async fn unsupported_records_explicit_state_without_reading_or_clearing_old_cursor() {
        let (store, _, mut scan) = fixture();
        scan.query.cursor = Some("original".into());
        assert_eq!(collect(&store, None, &mut scan).await.unwrap(), 0);
        collect(&store, None, &mut scan).await.unwrap();
        assert_eq!(scan.state, SettlementScanState::Unsupported);
        assert_eq!(scan.query.cursor.as_deref(), Some("original"));
        assert_eq!(*store.trace.lock().unwrap(), ["checkpoint"]);
    }
    #[tokio::test]
    async fn empty_page_with_cursor_continues_original_window_and_is_not_a_financial_result() {
        let (store, source, mut scan) = fixture();
        let original_window = scan.query.window.clone();
        assert_eq!(collect(&store, Some(&source), &mut scan).await.unwrap(), 0);
        assert!(!scan.complete);
        assert_eq!(scan.query.cursor.as_deref(), Some("next"));

        let mut recovered: SettlementScan =
            serde_json::from_str(&serde_json::to_string(&scan).unwrap()).unwrap();
        collect(&store, Some(&source), &mut recovered).await.unwrap();
        assert!(recovered.complete);
        assert!(source.queries.lock().unwrap().iter().all(|q| q.window == original_window));
        assert_eq!(
            *store.trace.lock().unwrap(),
            ["checkpoint", "read", "page", "checkpoint", "read", "page"]
        );
    }
    #[tokio::test]
    async fn checkpoint_failure_stops_before_http_and_page_failure_preserves_cursor() {
        for stage in 0..2 {
            let (mut store, source, mut scan) = fixture();
            store.fail_checkpoint = stage == 0;
            store.fail_page = stage == 1;
            assert!(collect(&store, Some(&source), &mut scan).await.is_err());
            assert!(store.stored.lock().unwrap().query.cursor.is_none());
            assert_eq!(source.queries.lock().unwrap().len(), stage);
        }
    }
}
