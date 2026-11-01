//! 库存调整详情：在调用方快照内组合库存、审批、人员与商品展示事实。

use std::sync::Arc;

use application_core::AuditActor;
use async_trait::async_trait;
use bpm::model::{ApprovalNodeExecution, ApprovalProcessInstance};
use erp_inventory::{InventoryService, StockAdjustment};
use erp_workflow::entity::document_registry::business_document::ApprovalDefinitionBinding;
use erp_workflow::entity::work_item::WorkItem;
use mongodb::Database;
use persistence_core::Executor;

use crate::Result;

mod approval;
mod detail;
mod projection;

pub use approval::load_approval_binding;
pub use projection::{RECENT_HISTORY_LIMIT, document_approval_view_with_history};

/// 审批动作沿用正式命令的资格与运行事实政策；不提供业务数据读取或写入。
#[async_trait]
pub trait InventoryAdjustmentApprovalPolicyPort: Send + Sync {
    /// 在调用方快照内判定当前操作人的提交资格。
    ///
    /// # 参数
    /// * `adjustment` - 已授权的调整单。
    /// * `actor` - 当前操作人。
    /// * `executor` - 调用方读取快照。
    ///
    /// # 返回
    /// 明确拒绝为 `false`，资格通过为 `true`。
    ///
    /// # 错误
    /// 授权读取失败时返回错误。
    async fn actor_can_submit(
        &self,
        adjustment: &StockAdjustment,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<bool>;

    /// 在调用方快照内判定当前操作人的普通撤回资格。
    ///
    /// # 参数
    /// * `instance` - 精确审批实例。
    /// * `actor` - 当前操作人。
    /// * `executor` - 调用方读取快照。
    ///
    /// # 返回
    /// 明确拒绝为 `false`，资格通过为 `true`。
    ///
    /// # 错误
    /// 授权或冻结事实读取失败时返回错误。
    async fn actor_can_cancel(
        &self,
        instance: &ApprovalProcessInstance,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<bool>;

    /// 校验精确实例与冻结定义绑定。
    ///
    /// # 参数
    /// * `instance` - 已加载实例。
    /// * `binding` - 调整单冻结绑定。
    ///
    /// # 返回
    /// 身份一致时成功。
    ///
    /// # 错误
    /// 身份或版本不一致时返回冲突。
    fn ensure_instance_binding(
        &self,
        instance: &ApprovalProcessInstance,
        binding: &ApprovalDefinitionBinding,
    ) -> Result<()>;

    /// 校验精确实例与调整单主题版本。
    ///
    /// # 参数
    /// * `instance` - 已加载实例。
    /// * `id` - 调整单主键。
    /// * `version` - 期望主题版本。
    ///
    /// # 返回
    /// 身份一致时成功。
    ///
    /// # 错误
    /// 身份或版本不一致时返回冲突。
    fn ensure_instance_subject(
        &self,
        instance: &ApprovalProcessInstance,
        id: &str,
        version: u32,
    ) -> Result<()>;

    /// 校验开放任务与当前审批运行事实。
    ///
    /// # 参数
    /// * `task` - 当前开放任务。
    /// * `instance` - 精确审批实例。
    /// * `execution` - 当前执行。
    /// * `organization` - 冻结责任组织。
    ///
    /// # 返回
    /// 身份一致时成功。
    ///
    /// # 错误
    /// 身份、任务责任或政策不一致时返回冲突或政策错误。
    fn ensure_open_task_identity(
        &self,
        task: &WorkItem,
        instance: &ApprovalProcessInstance,
        execution: &ApprovalNodeExecution,
        organization: &str,
    ) -> Result<()>;
}

/// 库存调整跨域只读服务。
#[derive(Clone)]
pub struct InventoryAdjustmentReadService {
    db: Database,
    inventory: Arc<InventoryService>,
    policy: Arc<dyn InventoryAdjustmentApprovalPolicyPort>,
}

impl InventoryAdjustmentReadService {
    /// 装配领域读取和正式命令共用的审批政策。
    ///
    /// # 参数
    /// * `db` - 各领域共用数据库。
    /// * `inventory` - 已注入仓库范围与展示事实端口的库存服务。
    /// * `policy` - 正式命令审批动作政策。
    ///
    /// # 返回
    /// 返回未执行 I/O 的读取服务。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn new(
        db: Database,
        inventory: Arc<InventoryService>,
        policy: Arc<dyn InventoryAdjustmentApprovalPolicyPort>,
    ) -> Self {
        Self { db, inventory, policy }
    }
}
