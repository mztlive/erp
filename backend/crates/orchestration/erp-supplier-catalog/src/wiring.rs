//! 供应目录消费的已装配领域服务与正式任务操作合同。

use std::sync::Arc;

use application_core::AuditActor;
use async_trait::async_trait;
use erp_catalog::{CatalogAccess, CatalogService};
use erp_core::ids::WorkItemId;
use erp_identity::AccessTarget;
use erp_permissions::internal;
use erp_supplier::SupplierAccess;
use erp_supply::service::supplier_offering::{OfferingAccess, SupplierOfferingService};
use erp_workflow::entity::work_item::{SupplierPortalReviewIdentity, SupplierPortalReviewTaskData, WorkItem};
use erp_workflow::ports::WorkflowAuthorizationPort;
use erp_workflow::{Result, WorkItemService};
use persistence_core::Executor;

/// 应用装配的目录用例依赖；所有范围政策仍由拥有领域执行。
pub struct CatalogServices {
    /// 已装配审计、文件资产与范围 Port 的商品服务。
    pub catalog: CatalogService,
    /// 使用同一身份政策的商品对象授权。
    pub catalog_access: CatalogAccess,
    /// 已装配范围 Port 的供给服务。
    pub offerings: SupplierOfferingService,
    /// 使用同一身份政策的供给对象授权。
    pub offering_access: OfferingAccess,
    /// 使用同一身份政策的供应商对象授权。
    pub suppliers: SupplierAccess,
    /// 正式工作项操作；每次调用沿用业务事务执行器。
    pub work_items: Arc<dyn CatalogWorkItems>,
}

/// 目录、门户和集成治理实际消费的工作项操作。
#[async_trait]
pub trait CatalogWorkItems: Send + Sync {
    /// 由 identity 校验既有任务详情范围和当前分配。
    /// # 参数
    /// actor 为当前账号，id 为正式任务，executor 为原业务执行器。
    /// # 返回
    /// 任务范围和当前分配均成立时成功。
    /// # 错误
    /// 账号、范围、分配或身份设施失效时返回工作流原错误。
    async fn require_assigned(&self, actor: &AuditActor, id: &str, executor: &mut dyn Executor)
    -> Result<()>;

    /// 在当前快照检查原任务完整办理资格，允许已完成结果回放。
    /// # 参数
    /// actor、id 为当前账号和真实任务，executor 为原事务。
    /// # 返回
    /// 完整资格与真实当前指派均成立时成功。
    /// # 错误
    /// 资格撤销、指派变化、任务不存在或授权设施失败时拒绝。
    async fn require_result_access(
        &self,
        actor: &AuditActor,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<()>;

    /// 校验当前账号、执行权限、对象范围和正式任务责任。
    /// # 参数
    /// 当前操作人、正式任务和业务事务执行器。
    /// # 返回
    /// 授权通过。
    /// # 错误
    /// 保留工作流领域的原始拒绝或基础设施错误。
    async fn ensure_domain_decision_access(
        &self,
        actor: &AuditActor,
        item: &WorkItem,
        executor: &mut dyn Executor,
    ) -> Result<()>;
    /// 创建供应商申请的正式审核任务。
    /// # 参数
    /// 已冻结任务身份、申请事实和同一事务执行器。
    /// # 返回
    /// 已持久化的任务。
    /// # 错误
    /// 当前审核人不具资格或持久化失败时拒绝。
    async fn create_supplier_portal_review(
        &self,
        id: WorkItemId,
        data: SupplierPortalReviewTaskData,
        executor: &mut dyn Executor,
    ) -> Result<WorkItem>;
    /// 重验申请、主题版本、任务版本与当前处理人。
    /// # 参数
    /// 精确任务身份、当前操作人和同一事务执行器。
    /// # 返回
    /// 已核验的任务。
    /// # 错误
    /// 身份、版本或权限失效时拒绝。
    async fn validate_supplier_portal_review(
        &self,
        identity: SupplierPortalReviewIdentity<'_>,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<WorkItem>;
    /// 完成申请的原正式审核任务。
    /// # 参数
    /// 精确任务身份、当前操作人和同一事务执行器。
    /// # 返回
    /// 已完成的任务。
    /// # 错误
    /// 身份、版本、权限或写入失败时拒绝。
    async fn complete_supplier_portal_review(
        &self,
        identity: SupplierPortalReviewIdentity<'_>,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<WorkItem>;
    /// 撤回原申请的正式审核任务。
    /// # 参数
    /// 精确任务身份、撤回人、原原因和同一事务执行器。
    /// # 返回
    /// 已撤回的任务。
    /// # 错误
    /// 身份、版本或写入失败时拒绝。
    async fn withdraw_supplier_portal_review(
        &self,
        identity: SupplierPortalReviewIdentity<'_>,
        actor: &str,
        reason: &str,
        executor: &mut dyn Executor,
    ) -> Result<WorkItem>;
}

#[async_trait]
impl<A: WorkflowAuthorizationPort> CatalogWorkItems for WorkItemService<A> {
    async fn require_assigned(
        &self,
        actor: &AuditActor,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        self.authorization.require_assigned(actor, id, executor).await.map(|_| ())
    }

    async fn require_result_access(
        &self,
        actor: &AuditActor,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let task = self.authorization.require_assigned(actor, id, executor).await?;
        self.authorization
            .service
            .require(
                actor,
                &internal::WORK_ITEM_EXECUTE,
                AccessTarget::Task { id: task.base.id, version: task.base.version.to_string() },
                executor,
            )
            .await
            .map_err(Into::into)
    }

    async fn ensure_domain_decision_access(
        &self,
        actor: &AuditActor,
        item: &WorkItem,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        self.ensure_domain_decision_access(actor, item, executor).await
    }
    async fn create_supplier_portal_review(
        &self,
        id: WorkItemId,
        data: SupplierPortalReviewTaskData,
        executor: &mut dyn Executor,
    ) -> Result<WorkItem> {
        self.create_supplier_portal_review(id, data, executor).await
    }
    async fn validate_supplier_portal_review(
        &self,
        identity: SupplierPortalReviewIdentity<'_>,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<WorkItem> {
        self.validate_supplier_portal_review(identity, actor, executor).await
    }
    async fn complete_supplier_portal_review(
        &self,
        identity: SupplierPortalReviewIdentity<'_>,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<WorkItem> {
        self.complete_supplier_portal_review(identity, actor, executor).await
    }
    async fn withdraw_supplier_portal_review(
        &self,
        identity: SupplierPortalReviewIdentity<'_>,
        actor: &str,
        reason: &str,
        executor: &mut dyn Executor,
    ) -> Result<WorkItem> {
        self.withdraw_supplier_portal_review(identity, actor, reason, executor).await
    }
}
