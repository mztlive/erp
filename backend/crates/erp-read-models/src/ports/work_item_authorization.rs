//! 详情读取消费的正式任务授权事实；授权执行仍由工作流拥有。

use application_core::AuditActor;
use async_trait::async_trait;
use erp_workflow::WorkItemType;
use erp_workflow::dto::work_item::WorkItemAllowedAction;

/// 原工作流授权结果中两条详情实际消费的字段。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthorizedTaskFact {
    /// 正式任务类型，用于详情核对所属业务处理器。
    pub work_item_type: WorkItemType,
    /// 正式关联的业务对象类型。
    pub business_object_type: String,
    /// 正式关联的业务对象身份。
    pub business_object_id: String,
    /// 任务冻结的业务版本，不等同于任务自身版本。
    pub subject_version: String,
    /// 工作流根据当前责任和对象权限计算的动作。
    pub allowed_actions: Vec<WorkItemAllowedAction>,
}

/// 由组合根注入，详情在原读取位置请求正式工作流授权。
///
/// 不接受 Executor：原 authorize_work_item 独立执行 NoTransaction 读取，
/// 该端口不改变其快照边界，也不替代详情后续的任务重读和领域动作判断。
#[async_trait]
pub trait WorkItemAuthorizationReadPort: Send + Sync {
    /// 授权后返回该详情需要的最小正式任务事实。
    ///
    /// # Errors
    /// 保留工作流原 NotFound、Forbidden、Conflict 与底层读取错误。
    async fn authorize(&self, id: &str, actor: &AuditActor) -> erp_workflow::Result<AuthorizedTaskFact>;
}
