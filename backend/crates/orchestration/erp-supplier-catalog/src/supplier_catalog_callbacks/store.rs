use std::future::Future;

use application_core::AuditActor;
use erp_audit::AuditActorLogs;
use erp_audit::execution::persist_log;
use erp_catalog::CatalogService;
use erp_core::common::time::Instant;
use erp_integration::entity::supplier_callback::SupplierCallbackReceipt;
use erp_integration::repository::SupplierCallbackExt;
use erp_supply::entity::supplier_api::SupplierApiConnection;
use erp_supply::repository::supplier_catalog::SupplierCatalogExt;
use mongodb::Database;
use persistence_core::{Executor, NoTransaction, Transactional};

use super::query::CatalogUpdate;
use super::{Context, SupplierCatalogCallbackProcess, after};
use crate::supplier_connection_execution::{guard_catalog_refresh, save_catalog_observation};
use crate::{Error, Result};

impl SupplierCatalogCallbackProcess {
    /// 按候选记录版本领取到期回调。
    /// # 参数
    /// 队列读取的候选接收记录。
    /// # 返回
    /// 取得租约的记录，或尚未到期。
    /// # 错误
    /// 并发领取、非法提示或保存失败时拒绝。
    pub(super) async fn claim(
        &self,
        candidate: &SupplierCallbackReceipt,
    ) -> Result<Option<SupplierCallbackReceipt>> {
        let mut receipt = candidate.clone();
        if !receipt.claim_catalog(Instant::now())? {
            return Ok(None);
        }
        // Repository::update 按 base.version CAS；多个进程只能有一个取得该版本。
        self.db.supplier_callback_receipts().update(&mut receipt, &mut NoTransaction).await?;
        Ok(Some(receipt))
    }

    /// 同事务应用一个规格并推进回调检查点。
    /// # 参数
    /// 当前租约、补查上下文和当前事实。
    /// # 返回
    /// 已提交的新版本检查点。
    /// # 错误
    /// 租约失效、配置变化或任一写入失败时回滚。
    pub(super) async fn save_step(
        &self,
        receipt: &SupplierCallbackReceipt,
        context: &Context,
        update: Option<CatalogUpdate>,
    ) -> Result<SupplierCallbackReceipt> {
        let (process, receipt) = (self.clone(), receipt.clone());
        let (connection, actor) = (context.connection.clone(), context.actor.clone());
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    process.save_step_stage(&receipt, &connection, &actor, update, executor).await
                })
            })
            .await
    }

    /// 当前采集资格必须在写入快照内成立，事务外预检不能代替保存授权。
    async fn save_step_stage(
        &self,
        receipt: &SupplierCallbackReceipt,
        connection: &SupplierApiConnection,
        actor: &AuditActor,
        update: Option<CatalogUpdate>,
        executor: &mut dyn Executor,
    ) -> Result<SupplierCallbackReceipt> {
        guard_catalog_refresh(&self.db, connection, actor, &self.authorization, executor).await?;
        let mut current = load_claimed(&self.db, receipt, executor).await?;

        advance_after_apply(&mut current, async {
            if let Some(update) = update {
                let id = update.id().to_owned();
                apply_update(&self.db, &self.catalog, update, actor, executor).await?;
                Ok(Some(id))
            } else {
                Ok(None)
            }
        })
        .await?;

        current.next_attempt_at = Some(after(300));
        self.db.supplier_callback_receipts().update(&mut current, executor).await?;
        Ok(current)
    }

    /// 完成或交接当前有界批次。
    /// # 参数
    /// 本轮最后已提交的回调版本。
    /// # 返回
    /// 已结算的新版本。
    /// # 错误
    /// 并发变更或保存失败时拒绝。
    pub(super) async fn settle(&self, receipt: &SupplierCallbackReceipt) -> Result<SupplierCallbackReceipt> {
        let mut settled = receipt.clone();
        settled.settle_catalog(Instant::now())?;
        self.db.supplier_callback_receipts().update(&mut settled, &mut NoTransaction).await?;
        Ok(settled)
    }

    /// 保留原检查点并延迟失败通知。
    /// # 参数
    /// 最后已提交版本和脱敏失败原因。
    /// # 返回
    /// 延期保存成功。
    /// # 错误
    /// 版本已变化或数据库失败时拒绝覆盖。
    pub(super) async fn defer(&self, receipt: &SupplierCallbackReceipt, summary: &str) -> Result<()> {
        let mut deferred = receipt.clone();
        deferred.processing_error = Some(summary.into());
        deferred.next_attempt_at = Some(after(60));
        self.db.supplier_callback_receipts().update(&mut deferred, &mut NoTransaction).await?;
        Ok(())
    }
}

async fn load_claimed(
    db: &Database,
    receipt: &SupplierCallbackReceipt,
    executor: &mut dyn Executor,
) -> Result<SupplierCallbackReceipt> {
    let current = db
        .supplier_callback_receipts()
        .find_by_id(&receipt.base.id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("商品回调不存在".into()))?;
    if current.status != "received"
        || current.base.version != receipt.base.version
        || current.connection_id != receipt.connection_id
        || current.next_attempt_at.is_none_or(|until| until <= Instant::now())
    {
        return Err(Error::ConflictError("商品回调领取已失效，请等待下一轮恢复".into()));
    }
    Ok(current)
}

async fn apply_update(
    db: &Database,
    catalog: &CatalogService,
    update: CatalogUpdate,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<()> {
    match update {
        CatalogUpdate::Offer(item) => {
            if db
                .supplier_catalog_items()
                .find_by_id(&item.base.id, executor)
                .await?
                .is_some_and(|current| current.observed_at > item.observed_at)
            {
                return Err(Error::ConflictError("已有更新商品观察，请重新查询".into()));
            }
            save_catalog_observation(db, catalog, *item, actor, executor).await
        },
        CatalogUpdate::Regions { id, regions } => {
            let mut item = db
                .supplier_catalog_items()
                .find_by_id(&id, executor)
                .await?
                .ok_or_else(|| Error::NotFound("城市回调对应目录已不存在，请先同步目录".into()))?;
            item.apply_service_regions(regions)?;
            db.supplier_catalog_items().update(&mut item, executor).await?;

            let audit =
                actor.clone().resource_log("supplier_catalog.regions.apply", "supplier_catalog_item", id)?;
            persist_log(db, &audit, executor).await?;
            Ok(())
        },
    }
}

/// 只有业务应用成功后才推进提示；失败保留上次已提交的进度。
async fn advance_after_apply(
    receipt: &mut SupplierCallbackReceipt,
    applied: impl Future<Output = Result<Option<String>>>,
) -> Result<()> {
    let single = receipt.catalog_hint()?.is_some_and(|hint| hint.target_kind == "sku");
    if let Some(id) = applied.await? {
        receipt.advance_catalog(&id, single)?;
    } else {
        receipt.finish_catalog_hint()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use erp_core::ids::SupplierApiConnectionId;
    use erp_integration::entity::supplier_callback::SupplierRefreshHint;

    use super::*;

    #[tokio::test]
    async fn callback_commit_requires_current_snapshot_qualification_before_checkpoint_reads_or_writes() {
        use std::sync::Arc;

        use erp_catalog::CatalogAccess;
        use erp_catalog::ports::{EmptyFileAssetFacts, FailClosedAuditPort};
        use erp_core::AccountKind;
        use erp_core::ids::SupplierAccountId;
        use erp_identity::testing::unavailable_authorization;
        use erp_supply::connector_registry::SupplierConnectorRegistry;
        use erp_supply::entity::supplier_api::{
            SupplierApiConnectionData, SupplierApiConnectionStatus, SupplierConnectorType,
        };
        use mongodb::Client;
        use persistence_core::ReadExecutor;
        use persistence_core::testing::SnapshotExecutor;

        let db = Client::with_uri_str("mongodb://127.0.0.1:1/?serverSelectionTimeoutMS=1")
            .await
            .unwrap()
            .database("callback_commit_authorization");
        let authorization = unavailable_authorization();
        let catalog = CatalogService::new(
            db.clone(),
            Arc::new(FailClosedAuditPort),
            Arc::new(EmptyFileAssetFacts),
            CatalogAccess::new(db.clone(), authorization.clone()),
        );
        let process = SupplierCatalogCallbackProcess::new(
            db,
            Arc::new(SupplierConnectorRegistry::default()),
            authorization,
            catalog,
        );

        let connection = SupplierApiConnection::new(
            SupplierApiConnectionId::new("connection"),
            SupplierApiConnectionData {
                supplier_id: SupplierAccountId::new("supplier"),
                connection_code: "CONNECTION".into(),
                connector_type: SupplierConnectorType::Dangaoshushu,
                endpoint_reference: "endpoint".into(),
                credential_reference: None,
                rate_limit_policy: None,
                status: SupplierApiConnectionStatus::Active,
            },
            "actor",
        )
        .unwrap();

        let receipt = SupplierCallbackReceipt::received(
            "receipt".into(),
            SupplierApiConnectionId::new("connection"),
            Instant::from_unix_secs(1),
            "product".into(),
            "cipher".into(),
            vec![SupplierRefreshHint {
                topic: "product".into(),
                target_kind: "sku".into(),
                object_id: "sku".into(),
                product_id: None,
                funds_transaction_id: None,
            }],
        )
        .unwrap();
        let actor = AuditActor::new("actor".into(), "operator".into(), AccountKind::Admin);

        assert!(matches!(
            process.save_step_stage(&receipt, &connection, &actor, None, &mut SnapshotExecutor::default()).await,
            Err(Error::Internal(message)) if message == "测试授权设施不可用"
        ));
        assert!(matches!(
            process.save_step_stage(&receipt, &connection, &actor, None, &mut ReadExecutor::default()).await,
            Err(Error::ValidationError(message)) if message == "写入授权必须使用快照事务"
        ));
    }

    #[tokio::test]
    async fn catalog_checkpoint_advances_only_after_successful_business_application() {
        let mut receipt = SupplierCallbackReceipt::received(
            "receipt".into(),
            SupplierApiConnectionId::new("connection"),
            Instant::from_unix_secs(1),
            "product".into(),
            "cipher".into(),
            vec![SupplierRefreshHint {
                topic: "product".into(),
                target_kind: "product".into(),
                object_id: "product".into(),
                product_id: None,
                funds_transaction_id: None,
            }],
        )
        .unwrap();

        assert!(advance_after_apply(&mut receipt, async { Ok(None) }).await.is_err());
        advance_after_apply(&mut receipt, async { Ok(Some("item-1".into())) }).await.unwrap();
        let saved = receipt.catalog_progress.clone();

        assert!(
            advance_after_apply(&mut receipt, async {
                Err(Error::ConflictError("business write failed".into()))
            })
            .await
            .is_err()
        );
        assert_eq!(receipt.catalog_progress, saved);
        assert_eq!(receipt.status, "received");

        advance_after_apply(&mut receipt, async { Ok(Some("item-2".into())) }).await.unwrap();
        advance_after_apply(&mut receipt, async { Ok(None) }).await.unwrap();
        assert!(receipt.settle_catalog(Instant::from_unix_secs(3)).unwrap());
    }
}
