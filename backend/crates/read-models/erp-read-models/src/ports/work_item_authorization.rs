//! 详情读取消费的正式任务授权事实；授权执行仍由工作流拥有。

use application_core::AuditActor;
use async_trait::async_trait;
use erp_identity::CompiledAccess;
use erp_workflow::WorkItemType;
use erp_workflow::dto::work_item::WorkItemAllowedAction;
use persistence_core::Executor;

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
/// 任务资格、业务事实与详情投影必须使用调用方的同一快照执行器。
/// 实现不得另开事务，也不得把授权结果带到其他读取阶段复用。
#[async_trait]
pub trait WorkItemAuthorizationReadPort: Send + Sync {
    /// 详情需要正式任务权限时，按当前操作者在原快照读取任务类型、业务对象、冻结版本、可执行动作及待读取方复验的凭证。
    ///
    /// # 参数
    /// * `id` - 正式任务身份
    /// * `executor` - 读取用例的原快照事务执行器
    ///
    /// # 返回
    /// 返回任务类型、业务对象、冻结版本、允许动作及尚未交付的读取凭证。
    /// 调用方完成全部业务资料读取后，必须用原共享授权服务验证该凭证。
    ///
    /// # 错误
    /// 保留工作流原 `NotFound`、`Forbidden`、`Conflict` 与底层读取错误。
    async fn authorize(
        &self,
        id: &str,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> erp_workflow::Result<(AuthorizedTaskFact, CompiledAccess)>;
}
