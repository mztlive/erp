//! 后台按固定成员分块生成草稿，进度与草稿共用短事务。

use application_core::AuditActor;
use erp_supply::catalog_adoption::{
    BatchAdoptionExt, CatalogAdoptionBatch, PreparationAction, PreparationStatus, SelectionView,
    insert_prepared_items, selection_page, selection_view,
};
use erp_supply::repository::supplier_catalog;
use persistence_core::{Error as PersistenceError, Executor, NoTransaction, Transactional};

use super::SupplierCatalogAdoptionProcess;
use super::batch::new_batch_item;
use crate::{Error, Result};

impl SupplierCatalogAdoptionProcess {
    /// 读取准备摘要，允许未完成批次，仍校验当前连接和供应商权限。
    /// # 参数
    /// connection、id 为连接和批次身份；actor 为当前认证用户。
    /// # 返回
    /// 不含成员资料的批次头。
    /// # 错误
    /// 不存在、旧未完成孤立批次或越权时拒绝。
    pub async fn preparation_batch(
        &self,
        connection: &str,
        id: &str,
        actor: &AuditActor,
    ) -> Result<CatalogAdoptionBatch> {
        let (connection, id, actor) = (connection.to_owned(), id.to_owned(), actor.clone());
        self.with_snapshot(move |process, executor| {
            Box::pin(async move { process.read_preparation_batch(&connection, &id, &actor, executor).await })
        })
        .await
    }

    /// 在调用方原事务内先授权，再读取并核对批次所属连接和供应商。
    pub(super) async fn read_preparation_batch(
        &self,
        connection: &str,
        id: &str,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<CatalogAdoptionBatch> {
        let source = self.batch_access(connection, actor, executor).await?;
        self.db
            .catalog_adoption_batches()
            .find_by_id(id, executor)
            .await?
            .filter(|batch| {
                (batch.selection_complete || batch.preparation.is_some())
                    && batch.connection_id == connection
                    && batch.supplier_id == source.supplier_id.as_ref()
            })
            .ok_or_else(|| Error::NotFound("入库批次不存在或不在当前数据范围".into()))
    }

    /// 返回商品库选择条所需轻量成员，避免下载全部配置资料。
    /// # 参数
    /// 连接、批次与当前认证用户。
    /// # 返回
    /// 完成批次的成员身份、名称和商品数量。
    /// # 错误
    /// 越权、未准备完成或数据库失败时拒绝。
    pub async fn batch_selection_view(
        &self,
        connection: &str,
        id: &str,
        actor: &AuditActor,
    ) -> Result<SelectionView> {
        let (connection, id, actor) = (connection.to_owned(), id.to_owned(), actor.clone());
        self.with_snapshot(move |process, executor| {
            Box::pin(async move {
                let batch = process.load_batch(&connection, &id, &actor, executor).await?;
                Ok(selection_view(&process.db, batch, executor).await?)
            })
        })
        .await
    }

    /// 重试或取消原准备任务，不变更其冻结成员及已完成部分。
    /// # 参数
    /// 连接、批次、动作和认证用户。
    /// # 返回
    /// 原子更新后的准备摘要。
    /// # 错误
    /// 权限、状态或并发冲突时拒绝。
    pub async fn control_preparation(
        &self,
        connection: &str,
        id: &str,
        action: PreparationAction,
        actor: &AuditActor,
    ) -> Result<CatalogAdoptionBatch> {
        let (process, connection, id, actor) =
            (self.clone(), connection.to_owned(), id.to_owned(), actor.clone());
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let mut batch =
                        process.read_preparation_batch(&connection, &id, &actor, executor).await?;
                    batch
                        .preparation
                        .as_mut()
                        .ok_or_else(|| Error::ConflictError("本批次没有准备任务".into()))?
                        .apply(action)?;
                    process.db.catalog_adoption_batches().update(&mut batch, executor).await?;
                    Ok(batch)
                })
            })
            .await
    }

    /// 由统一后台执行器持续处理有限分块，崩溃后从持久化进度继续。
    /// # 参数
    /// `id` 为已登记批次身份。
    /// # 返回
    /// 已完成或不再需要运行时成功。
    /// # 错误
    /// 权限、来源变化或存储错误时保留原进度；明确失败进入可重试状态。
    pub async fn prepare_batch(&self, id: &str) -> Result<()> {
        loop {
            let Some(original) =
                self.db.catalog_adoption_batches().find_by_id(id, &mut NoTransaction).await?
            else {
                return Ok(());
            };
            if original.preparation.as_ref().is_none_or(|state| !state.runnable()) {
                return Ok(());
            }

            let (process, batch) = (self.clone(), original.clone());
            let outcome: Result<()> = self
                .db
                .client()
                .clone()
                .with_transaction(move |executor| {
                    Box::pin(async move { process.prepare_chunk(batch, executor).await })
                })
                .await;

            if let Err(error) = outcome {
                // 未知提交和并发冲突只重新读取原子进度，不能重放未查明的一块。
                if !matches!(
                    error,
                    Error::OutcomeUnknown(_) | Error::TransientTransaction(_) | Error::ReceiptDuplicate(_)
                ) {
                    self.fail_preparation(original, &error).await?;
                }
                return Err(error);
            }
        }
    }

    /// 在同一事务重验权限、读取成员、批量写入并推进版本。
    async fn prepare_chunk(&self, original: CatalogAdoptionBatch, executor: &mut dyn Executor) -> Result<()> {
        let Some(mut batch) =
            self.db.catalog_adoption_batches().find_by_id(&original.base.id, executor).await?
        else {
            return Ok(());
        };
        if batch.preparation.as_ref().is_none_or(|state| !state.runnable()) {
            return Ok(());
        }

        let actor = self.batch_actor(&batch.actor_id, executor).await?;
        let source = self.batch_access(&batch.connection_id, &actor, executor).await?;
        self.ensure_source(&source)?;
        if batch.supplier_id != source.supplier_id.as_ref() {
            return Err(Error::Forbidden("原批次所属供应商已变更，请取消后重新选择".into()));
        }

        let state = batch.preparation.as_ref().ok_or_else(|| Error::Internal("准备状态缺失".into()))?;
        let page = selection_page(&self.db, &batch.base.id, state.next_page, executor)
            .await?
            .ok_or_else(|| Error::Internal("冻结的成员分块缺失，请联系管理员".into()))?;
        let items =
            supplier_catalog::selection_sources(&self.db, &batch.connection_id, &page.item_ids, executor)
                .await?;
        if items.len() != page.item_ids.len() {
            return Err(Error::ValidationError(
                "部分已选商品已删除或不可访问，请恢复来源后重试，或取消本批重新选择".into(),
            ));
        }
        for item in &items {
            source.ensure_offering_source(&item.supplier_id)?;
        }

        let rows = items.into_iter().map(|item| new_batch_item(&batch, item)).collect::<Vec<_>>();
        let state = batch.preparation.as_mut().expect("已检查准备状态存在");
        state.advance(rows.len(), batch.selected_count)?;
        batch.selection_complete = state.status == PreparationStatus::Ready;
        // 先 CAS 锁定批次版本；并发取消或另一个 worker 胜出时整块回滚。
        self.db.catalog_adoption_batches().update(&mut batch, executor).await?;
        insert_prepared_items(&self.db, &rows, executor).await?;
        Ok(())
    }

    /// 只给仍处于原版本的任务记失败，不覆盖并发进度或取消。
    async fn fail_preparation(&self, mut original: CatalogAdoptionBatch, error: &Error) -> Result<()> {
        let state = original.preparation.as_mut().ok_or_else(|| Error::Internal("准备状态缺失".into()))?;
        state.fail(match error {
            Error::ValidationError(message)
            | Error::Forbidden(message)
            | Error::BusinessLogicError(message) => message.clone(),
            _ => "准备中断，已完成部分已保存，请重试；持续失败请联系管理员".into(),
        })?;
        match self.db.catalog_adoption_batches().update(&mut original, &mut NoTransaction).await {
            Ok(()) | Err(PersistenceError::OptimisticLockingError) => Ok(()),
            Err(error) => Err(error.into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use async_trait::async_trait;
    use erp_catalog::{CatalogAccess, CatalogService, EmptyFileAssetFacts, FailClosedAuditPort};
    use erp_core::AccountKind;
    use erp_identity::entity::access_control::ResolvedScope;
    use erp_identity::ports::{IdentityAuditPort, PreparedResourceAudit};
    use erp_identity::testing::{fixed_authorization, unavailable_authorization};
    use erp_identity::{AuthorizationService, RbacService, Result as IdentityResult};
    use erp_supplier::SupplierAccess;
    use erp_supply::service::supplier_offering::{OfferingAccess, SupplierOfferingService};
    use erp_workflow::WorkItemService;
    use erp_workflow::ports::WorkflowAuthorizationPort;
    use mongodb::Client;
    use persistence_core::ReadExecutor;
    use persistence_core::testing::SnapshotExecutor;

    use super::*;
    use crate::wiring::CatalogServices;

    struct NoAudit;
    #[async_trait]
    impl IdentityAuditPort for NoAudit {
        fn resource_log(
            &self,
            _: AuditActor,
            _: &str,
            _: &str,
            _: String,
        ) -> IdentityResult<PreparedResourceAudit> {
            panic!("准备摘要读取不得准备身份审计")
        }
        async fn persist(&self, _: &PreparedResourceAudit, _: &mut dyn Executor) -> IdentityResult<()> {
            panic!("准备摘要读取不得写身份审计")
        }
    }

    #[derive(Clone)]
    struct Workflows;
    impl WorkflowAuthorizationPort for Workflows {}

    /// 失败发生在授权阶段；本夹具不发起连接、批次或成员查询。
    async fn process(authorization: Arc<AuthorizationService>) -> SupplierCatalogAdoptionProcess {
        let db = Client::with_uri_str("mongodb://127.0.0.1:1/?serverSelectionTimeoutMS=1")
            .await
            .unwrap()
            .database("adoption_preparation_authorization_unit");
        let catalog_access = CatalogAccess::new(db.clone(), authorization.clone());
        let offering_access = OfferingAccess::new(db.clone(), authorization.clone());
        let services = CatalogServices {
            catalog: CatalogService::new(
                db.clone(),
                Arc::new(FailClosedAuditPort),
                Arc::new(EmptyFileAssetFacts),
                catalog_access.clone(),
            ),
            catalog_access,
            offerings: SupplierOfferingService::new(db.clone()).with_data_scope(offering_access.clone()),
            offering_access,
            suppliers: SupplierAccess::new(db.clone(), authorization.clone()),
            work_items: Arc::new(WorkItemService::new(
                db.clone(),
                Arc::new(RbacService::new(db.clone(), Arc::new(NoAudit))),
                authorization.clone(),
                Workflows,
            )),
        };
        SupplierCatalogAdoptionProcess::new(db, authorization, Arc::new(services))
    }

    #[tokio::test]
    async fn preparation_read_requires_snapshot_before_identity_or_business_reads() {
        let process = process(unavailable_authorization()).await;
        let actor = AuditActor::new("actor".into(), "actor".into(), AccountKind::Admin);
        for executor in [&mut NoTransaction as &mut dyn Executor, &mut ReadExecutor::default()] {
            let result = process.read_preparation_batch("connection", "batch", &actor, executor).await;
            assert!(matches!(result, Err(Error::ValidationError(_))));
        }

        let result = process
            .read_preparation_batch("connection", "batch", &actor, &mut SnapshotExecutor::default())
            .await;
        assert!(matches!(result, Err(Error::Internal(message)) if message == "测试授权设施不可用"));
    }

    #[tokio::test]
    async fn preparation_read_rejects_missing_adoption_qualification_before_loading_connection_or_batch() {
        let authorization = fixed_authorization(
            &[],
            ResolvedScope { role_clauses: Vec::new(), user_limit: None },
            Vec::new(),
            Vec::new(),
        )
        .unwrap();
        let process = process(authorization).await;
        let actor = AuditActor::new("actor".into(), "actor".into(), AccountKind::Admin);
        let result = process
            .read_preparation_batch("connection", "batch", &actor, &mut SnapshotExecutor::default())
            .await;
        assert!(matches!(result, Err(Error::Forbidden(_))));
    }
}
