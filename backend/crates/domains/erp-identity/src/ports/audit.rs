//! 身份命令把跨域资源审计交给消费方持久化的端口。

use std::num::NonZeroU32;

use application_core::{AuditActor, impl_audit_context};
use async_trait::async_trait;
use entity_core::BaseModel;
use erp_core::AccountKind;
use persistence_core::Executor;

use crate::error::Result;

/// 已校验并等待落库的成功资源审计。组合层将它转换为审计记录，在业务命令使用的同一执行器上写入。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedResourceAudit {
    /// 审计记录的稳定标识，用于存储和追溯。
    pub id: String,
    /// 构造审计记录时保存的乐观锁版本。
    pub version: u64,
    /// 审计记录的创建时间戳。
    pub created_at: u64,
    /// 审计记录快照的最后更新时间戳。
    pub updated_at: u64,
    /// 持久化模型快照中的软删除标记。
    pub deleted_at: u64,
    /// 执行操作的账号 ID，用于关联审计主体。
    pub actor_id: String,
    /// 操作发生时的登录账号，用于检索和展示。
    pub actor_account: String,
    /// 操作发生时捕获的安全显示名；未取得名称时留空。
    pub actor_name_snapshot: Option<String>,
    /// 串联认证请求和审计记录的标识；无请求时留空。
    pub request_id: Option<String>,
    /// 同一业务命令产生多条审计时的顺序，从 1 开始。
    pub event_sequence: NonZeroU32,
    /// 操作人账号类型，用于区分后台、门户等身份。
    pub actor_type: AccountKind,
    /// 审计目录中标识本次业务操作的动作名。
    pub action: String,
    /// 被操作资源的业务类型，用于分类查询。
    pub resource_type: String,
    /// 被操作的资源 ID；无具体对象时留空。
    pub resource_id: Option<String>,
    /// 本次操作是否成功；身份用例通过此端口准备成功事件。
    pub success: bool,
    /// 补充本次操作上下文的可选业务说明。
    pub message: Option<String>,
}

impl_audit_context!(PreparedResourceAudit);

impl PreparedResourceAudit {
    /// 从已校验的审计字段与持久化元数据生成身份侧快照，供业务事务落库。名称和请求标识默认留空，序号从 1 开始，调用方可补齐上下文。
    #[allow(clippy::too_many_arguments)]
    pub fn from_validated(
        base: &BaseModel,
        actor_id: String,
        actor_account: String,
        actor_type: AccountKind,
        action: String,
        resource_type: String,
        resource_id: Option<String>,
        success: bool,
        message: Option<String>,
    ) -> Self {
        Self {
            id: base.id.clone(),
            version: base.version,
            created_at: base.created_at,
            updated_at: base.updated_at,
            deleted_at: base.deleted_at,
            actor_id,
            actor_account,
            actor_name_snapshot: None,
            request_id: None,
            event_sequence: NonZeroU32::MIN,
            actor_type,
            action,
            resource_type,
            resource_id,
            success,
            message,
        }
    }
}

/// 在业务事务前准备审计事实，并沿调用方执行器写入。
#[async_trait]
pub trait IdentityAuditPort: Send + Sync {
    /// 开启写事务前校验操作人与资源标识，并准备本次成功操作的审计快照。输入违反审计约束时提前返回错误。
    fn resource_log(
        &self,
        actor: AuditActor,
        action: &str,
        resource_type: &str,
        resource_id: String,
    ) -> Result<PreparedResourceAudit>;

    /// 将准备好的审计写入业务命令的执行器，使审计与业务变更共用事务。持久化失败交回调用方处理。
    async fn persist(&self, audit: &PreparedResourceAudit, executor: &mut dyn Executor) -> Result<()>;
}
