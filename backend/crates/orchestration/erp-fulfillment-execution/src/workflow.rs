//! 应用注入的单据绑定、材料授权与工作项决定能力。

use std::sync::Arc;

use application_core::AuditActor;
use async_trait::async_trait;
use bpm::model::ApprovalProcessInstance;
use erp_approval_runtime::ApprovalRuntimePort;
use erp_inventory::StockAdjustment;
use erp_read_models::workbench::authorize_material_transfer;
use erp_support::FileAsset;
use erp_workflow::entity::approval_integration::ApprovalSubjectSnapshot;
use erp_workflow::entity::document_registry::business_document::ApprovalDefinitionBinding;
use erp_workflow::entity::work_item::WorkItem;
use erp_workflow::repository::bpm::DefinitionGraph;
use erp_workflow::service::approval::binding::{
    BindPublishedDefinitionCommand, bind_published_definition_on_document_create,
};
use erp_workflow::service::approval::execution::PlannedWrites;
use erp_workflow::{WorkItemService, WorkflowAuthorizationPort};
use persistence_core::Executor;

use crate::Result;
use crate::inventory_adjustment::approval_prepare::{authorize_submit_with, revalidate_candidates_with};
pub use crate::inventory_adjustment::cancel_authorization::CancelAuthorization as StockAdjustmentCancelAuthorization;
use crate::inventory_adjustment::cancel_authorization::authorize_with;

/// 履约命令消费的工作流能力；具体授权、事实和审计由应用装配。
#[async_trait]
pub trait FulfillmentWorkflow: Send + Sync {
    /// 在原创建事务中绑定发布定义或记录无审批政策事实。
    ///
    /// # 参数
    /// `command`、`actor` 和 `executor` 属于原创建命令。
    /// # 返回
    /// 必须审批时返回绑定，无审批时返回空值。
    /// # 错误
    /// 定义、人员、范围、政策或审计校验失败时返回原错误。
    async fn bind_document(
        &self,
        command: &BindPublishedDefinitionCommand,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<Option<ApprovalDefinitionBinding>>;

    /// 在原事务中证明文件材料的转授资格。
    ///
    /// # 参数
    /// `actor` 为当前操作人，`assets` 为已经读取并校验的材料，`executor` 为原事务。
    /// # 返回
    /// 全部材料可转授时成功。
    /// # 错误
    /// 任一材料不可转授或授权读取失败时拒绝。
    async fn authorize_material_transfer(
        &self,
        actor: &AuditActor,
        assets: &[FileAsset],
        executor: &mut dyn Executor,
    ) -> Result<()>;

    /// 在原撤回事务内证明库存调整审批的提交人或运行管理员资格。
    ///
    /// # 参数
    /// `instance`、`actor` 和 `executor` 属于原撤回命令。
    /// # 返回
    /// 已核验的原提交人或运行管理员身份。
    /// # 错误
    /// 账号、动作、对象、冻结快照或策略无效时返回原错误。
    async fn authorize_stock_adjustment_cancel(
        &self,
        instance: &ApprovalProcessInstance,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<StockAdjustmentCancelAuthorization>;

    /// 在原启动事务内冻结材料并保存精确审批快照。
    ///
    /// # 参数
    /// `snapshot` 为原计划的冻结材料；`executor` 为原启动事务。
    /// # 返回
    /// 材料和快照持久化成功时返回。
    /// # 错误
    /// 材料授权、范围或持久化失败时返回原错误。
    async fn persist_start_snapshot(
        &self,
        snapshot: ApprovalSubjectSnapshot,
        executor: &mut dyn Executor,
    ) -> Result<()>;

    /// 在原事务内证明库存调整提交资格。
    ///
    /// # 参数
    /// `adjustment`、`actor` 和 `executor` 属于原提交命令。
    /// # 返回
    /// 账号、动作和对象范围均有效时成功。
    /// # 错误
    /// 任一资格失效时返回原错误。
    async fn authorize_stock_adjustment_submit(
        &self,
        adjustment: &StockAdjustment,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<()>;

    /// 在原启动事务内重验审批候选人和职责分离。
    ///
    /// # 参数
    /// `graph` 是冻结定义；其余为启动人、责任组织和原执行器。
    /// # 返回
    /// 全部候选有效时成功。
    /// # 错误
    /// 任一账号、权限、组织或职责分离不符时返回原错误。
    async fn revalidate_stock_adjustment_candidates(
        &self,
        graph: &DefinitionGraph,
        initiator_id: &str,
        organization_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<()>;

    /// 在原撤回事务中持久化通知所需材料副本。
    ///
    /// # 参数
    /// `writes` 为原撤回计划；`executor` 为原撤回事务。
    /// # 返回
    /// 精确材料副本持久化成功时返回。
    /// # 错误
    /// 材料资格或持久化失败时返回原错误。
    async fn persist_cancel_copies(&self, writes: &PlannedWrites, executor: &mut dyn Executor) -> Result<()>;

    /// 在原事务中重验正式工作项的决定资格。
    ///
    /// # 参数
    /// `actor`、`item` 与 `executor` 均属于当前正式命令。
    /// # 返回
    /// 当前账号、责任和对象资格有效时成功。
    /// # 错误
    /// 任一资格失效或事实查询失败时返回原错误。
    async fn ensure_domain_decision_access(
        &self,
        actor: &AuditActor,
        item: &WorkItem,
        executor: &mut dyn Executor,
    ) -> Result<()>;
}

/// 已装配的工作项服务与审批运行能力。
///
/// 身份服务由应用统一注入，业务步骤沿用调用方执行器。
pub struct FulfillmentWorkflowServices<A> {
    service: WorkItemService<A>,
    runtime: Arc<dyn ApprovalRuntimePort>,
}

impl<A> FulfillmentWorkflowServices<A> {
    /// 绑定调用方工作流服务和原材料步骤使用的审批运行能力。
    ///
    /// # 参数
    /// * `service` - 已注入原授权、业务事实和审计提供者的工作项服务。
    /// * `runtime` - 已注入共享身份和业务来源授权的审批运行能力。
    ///
    /// # 返回
    /// 返回不执行读取或写入的履约工作流能力。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn new(service: WorkItemService<A>, runtime: Arc<dyn ApprovalRuntimePort>) -> Self {
        Self { service, runtime }
    }
}

#[async_trait]
impl<A> FulfillmentWorkflow for FulfillmentWorkflowServices<A>
where
    A: WorkflowAuthorizationPort + Send + Sync + 'static,
{
    async fn bind_document(
        &self,
        command: &BindPublishedDefinitionCommand,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<Option<ApprovalDefinitionBinding>> {
        Ok(bind_published_definition_on_document_create(
            &self.service.authorization.service,
            &self.service.identity,
            &self.service.db,
            self.service.audit.as_ref(),
            command,
            actor,
            executor,
        )
        .await?)
    }

    async fn authorize_material_transfer(
        &self,
        actor: &AuditActor,
        assets: &[FileAsset],
        executor: &mut dyn Executor,
    ) -> Result<()> {
        Ok(authorize_material_transfer(
            &self.service.authorization.service,
            &self.service.db,
            actor,
            assets,
            executor,
        )
        .await?)
    }

    async fn authorize_stock_adjustment_cancel(
        &self,
        instance: &ApprovalProcessInstance,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<StockAdjustmentCancelAuthorization> {
        authorize_with(&self.service.authorization.service, &self.service.db, instance, actor, executor).await
    }

    async fn persist_start_snapshot(
        &self,
        snapshot: ApprovalSubjectSnapshot,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        Ok(self.runtime.persist_start_snapshot(&self.service.db, snapshot, executor).await?)
    }

    async fn authorize_stock_adjustment_submit(
        &self,
        adjustment: &StockAdjustment,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        authorize_submit_with(
            &self.service.authorization.service,
            &self.service.db,
            adjustment,
            actor,
            executor,
        )
        .await
    }

    async fn revalidate_stock_adjustment_candidates(
        &self,
        graph: &DefinitionGraph,
        initiator_id: &str,
        organization_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        revalidate_candidates_with(
            &self.service.authorization.service,
            &self.service.db,
            graph,
            initiator_id,
            organization_id,
            executor,
        )
        .await
    }

    async fn persist_cancel_copies(&self, writes: &PlannedWrites, executor: &mut dyn Executor) -> Result<()> {
        Ok(self.runtime.persist_cancel_copies(&self.service.db, writes, executor).await?)
    }

    async fn ensure_domain_decision_access(
        &self,
        actor: &AuditActor,
        item: &WorkItem,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        crate::supply_execution::task_authorization::require_assignee(
            &self.service.db,
            &self.service.authorization.service,
            actor,
            item,
            executor,
        )
        .await?;
        Ok(self.service.ensure_domain_decision_access(actor, item, executor).await?)
    }
}
