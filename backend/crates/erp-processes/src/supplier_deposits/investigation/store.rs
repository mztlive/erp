//! 账号级租约保护的证据收件、断点恢复及真实资金应用。

use std::num::NonZeroU32;
use std::sync::Arc;

use application_core::AuditActor;
use async_trait::async_trait;
use erp_audit::AuditActorLogs;
use erp_core::AccountKind;
use erp_core::common::time::Instant;
use erp_finance::entity::supplier_deposit::{
    FundsEvidencePage, FundsScanCheckpoint, FundsScanLease, FundsScanWindow,
};
use erp_finance::repository::SupplierDepositExt;
use erp_finance::repository::funds_evidence::{funds_scan_checkpoint, pending_funds_evidence};
use erp_finance::service::supplier_deposit::SupplierDepositService;
use erp_supply::entity::supplier_api::SupplierApiConnection;
use erp_supply::ports::connector::common::{Page, Snapshot};
use erp_supply::ports::connector::funds::{
    SupplierFundsBalance, SupplierFundsObservation, SupplierFundsQuery,
};
use id_generator::next_id;
use mongodb::Database;
use persistence_core::{Executor, NoTransaction, Transactional};
use tokio::sync::Mutex;

use super::convert;
use super::process::{FundsInvestigationRun, FundsInvestigationStore};
use crate::audit::persist_log;
use crate::{Error, Result};

pub(super) const SYSTEM_ACTOR: &str = "system:supplier-funds-investigation";

#[derive(Clone)]
pub(super) struct MongoFundsInvestigationStore {
    pub db: Database,
    pub account_id: String,
    pub connection_id: String,
    pub external_account_ref: String,
    pub lease: FundsScanLease,
    checkpoint: Arc<Mutex<Option<FundsScanCheckpoint>>>,
}

impl MongoFundsInvestigationStore {
    pub async fn claim(
        db: Database,
        connection: &SupplierApiConnection,
        external_account_ref: &str,
    ) -> Result<Option<Self>> {
        let account_id = connection
            .supplier_deposit_account_id
            .clone()
            .ok_or_else(|| Error::BusinessLogicError("资金调查缺少已核验的财务账号绑定".into()))?;
        let account = db
            .supplier_deposit_accounts()
            .find_by_id(&account_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("资金调查账户不存在".into()))?;
        if account.identity.supplier_id != connection.supplier_id.as_ref()
            || account.identity.external_account_ref != external_account_ref
            || connection.supplier_deposit_external_account_ref.as_deref() != Some(external_account_ref)
        {
            return Err(Error::Forbidden("资金读取账号与当前连接或财务账户归属不一致".into()));
        }
        let lease_db = db.clone();
        let lease_account = account_id.clone();
        let owner = next_id();
        let lease = db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    Ok::<_, Error>(
                        SupplierDepositService::new(lease_db)
                            .claim_funds_lease(&lease_account, &owner, Instant::now(), 300, executor)
                            .await?,
                    )
                })
            })
            .await?;
        Ok(lease.map(|lease| Self {
            db,
            account_id,
            connection_id: connection.base.id.clone(),
            external_account_ref: external_account_ref.into(),
            lease,
            checkpoint: Arc::new(Mutex::new(None)),
        }))
    }

    pub async fn fence(&self, executor: &mut dyn Executor) -> Result<()> {
        SupplierDepositService::new(self.db.clone())
            .assert_funds_lease(
                &self.account_id,
                &self.lease.owner_id,
                self.lease.fencing_token,
                Instant::now(),
                executor,
            )
            .await?;
        Ok(())
    }

    pub async fn check_lease(&self) -> Result<()> {
        let store = self.clone();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| Box::pin(async move { store.fence(executor).await }))
            .await
    }

    pub async fn release(&self) -> Result<()> {
        let store = self.clone();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    SupplierDepositService::new(store.db.clone())
                        .release_funds_lease(
                            &store.account_id,
                            &store.lease.owner_id,
                            store.lease.fencing_token,
                            Instant::now(),
                            executor,
                        )
                        .await?;
                    Ok::<_, Error>(())
                })
            })
            .await
    }

    async fn finish_checkpoint(&self) -> Result<()> {
        let store = self.clone();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    store.fence(executor).await?;
                    if let Some(checkpoint) =
                        funds_scan_checkpoint(&store.db, &store.account_id, executor).await?
                        && checkpoint.read_complete
                        && !checkpoint.finished
                    {
                        SupplierDepositService::new(store.db.clone())
                            .finish_funds_scan(&checkpoint.base.id, executor)
                            .await?;
                    }
                    Ok::<_, Error>(())
                })
            })
            .await
    }
}

#[async_trait]
impl FundsInvestigationStore for MongoFundsInvestigationStore {
    fn account_reference(&self) -> &str {
        &self.external_account_ref
    }

    async fn resume(&self) -> Result<FundsInvestigationRun> {
        self.check_lease().await?;
        let pending = pending_funds_evidence(&self.db, &self.account_id, 50, &mut NoTransaction).await?;
        let mut report = FundsInvestigationRun::default();
        for evidence in pending {
            report.merge(self.apply_received(evidence).await?);
        }
        self.finish_checkpoint().await?;
        Ok(report)
    }

    async fn query(&self, now: Instant) -> Result<SupplierFundsQuery> {
        let store = self.clone();
        let checkpoint = self
            .db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    store.fence(executor).await?;
                    let latest = funds_scan_checkpoint(&store.db, &store.account_id, executor).await?;
                    if let Some(checkpoint) = latest.as_ref().filter(|checkpoint| !checkpoint.finished) {
                        if checkpoint.read_complete {
                            return Err(Error::BusinessLogicError(
                                "原资金证据仍在恢复，暂不推进读取窗口".into(),
                            ));
                        }
                        return Ok(checkpoint.clone());
                    }
                    let account = store
                        .db
                        .supplier_deposit_accounts()
                        .find_by_id(&store.account_id, executor)
                        .await?
                        .ok_or_else(|| Error::NotFound("原资金账户不存在".into()))?;
                    let window = next_window(latest.as_ref(), account.base.created_at, now)?;
                    let key = format!("{}:{}", window.since.unix_secs(), window.through.unix_secs());
                    Ok::<_, Error>(
                        SupplierDepositService::new(store.db.clone())
                            .begin_funds_scan(&store.account_id, &key, window, executor)
                            .await?,
                    )
                })
            })
            .await?;
        if checkpoint.finished {
            return Err(Error::BusinessLogicError("当前资金窗口已完成读取，请等待下一采样时点".into()));
        }
        let query = SupplierFundsQuery {
            since: checkpoint.window.since,
            through: checkpoint.window.through,
            cursor: checkpoint.cursor.clone(),
            limit: NonZeroU32::new(50).expect("固定非零批次"),
        };
        *self.checkpoint.lock().await = Some(checkpoint);
        self.check_lease().await?;
        Ok(query)
    }

    async fn receive_page(
        &self,
        query: &SupplierFundsQuery,
        page: Page<Snapshot<SupplierFundsObservation>>,
    ) -> Result<usize> {
        let checkpoint = self
            .checkpoint
            .lock()
            .await
            .clone()
            .ok_or_else(|| Error::Internal("资金查询缺少原固定检查点".into()))?;
        let count = page.items.len();
        let record = FundsEvidencePage {
            window: FundsScanWindow { since: query.since, through: query.through },
            requested_cursor: query.cursor.clone(),
            complete: page.next.is_none(),
            next_cursor: page.next,
            observed_at: Instant::now(),
            source_revision: None,
            observations: page.items.into_iter().map(convert::observation).collect(),
        };
        let store = self.clone();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    store.fence(executor).await?;
                    SupplierDepositService::new(store.db.clone())
                        .save_funds_page(&checkpoint.base.id, checkpoint.base.version, record, executor)
                        .await?;
                    store.audit("funds.receive", executor).await
                })
            })
            .await?;
        Ok(count)
    }

    async fn observe_balance(&self, balance: Snapshot<SupplierFundsBalance>) -> Result<()> {
        if balance.value.account_reference != self.external_account_ref {
            return Err(Error::Forbidden("余额观察不属于当前资金账号".into()));
        }
        let record = convert::balance(balance);
        let store = self.clone();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    store.fence(executor).await?;
                    SupplierDepositService::new(store.db.clone())
                        .record_funds_balance_observation(&store.account_id, record, executor)
                        .await?;
                    store.audit("funds.balance", executor).await
                })
            })
            .await
    }
}

impl MongoFundsInvestigationStore {
    pub async fn audit(&self, action: &str, executor: &mut dyn Executor) -> Result<()> {
        let actor = AuditActor::new(SYSTEM_ACTOR.into(), "供应商资金调查".into(), AccountKind::Admin);
        let log = actor.resource_log(
            &format!("supplier_deposit.{action}"),
            "supplier_deposit",
            self.account_id.clone(),
        )?;
        persist_log(&self.db, &log, executor).await?;
        Ok(())
    }
}

fn next_window(
    previous: Option<&FundsScanCheckpoint>,
    created_at: u64,
    now: Instant,
) -> Result<FundsScanWindow> {
    let start = match previous.and_then(|checkpoint| checkpoint.completed_through) {
        Some(completed) => completed.unix_secs().saturating_sub(300),
        None => {
            i64::try_from(created_at).map_err(|_| Error::ValidationError("账户建立时间超出范围".into()))?
        },
    }
    .min(now.unix_secs().saturating_sub(1));
    let through = start
        .checked_add(86400)
        .ok_or_else(|| Error::ValidationError("资金窗口超出范围".into()))?
        .min(now.unix_secs());
    Ok(FundsScanWindow { since: Instant::from_unix_secs(start), through: Instant::from_unix_secs(through) })
}
