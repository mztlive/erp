//! 身份命令把跨域资源审计交给消费方持久化的端口。

use std::num::NonZeroU32;

use application_core::{AuditActor, impl_audit_context};
use async_trait::async_trait;
use entity_core::BaseModel;
use erp_core::AccountKind;
use persistence_core::Executor;

use crate::error::Result;

/// Prepared successful resource audit that identity can persist through a port.
///
/// Identity never depends on `erp-audit` types. Composition-root adapters convert
/// this fact into an `AuditLog` and write it on the same [`Executor`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedResourceAudit {
    /// Stable audit document id.
    pub id: String,
    /// Optimistic-lock version captured at construction.
    pub version: u64,
    /// Creation timestamp captured at construction.
    pub created_at: u64,
    /// Update timestamp captured at construction.
    pub updated_at: u64,
    /// Soft-delete marker captured at construction.
    pub deleted_at: u64,
    /// Actor account id.
    pub actor_id: String,
    /// Actor login account.
    pub actor_account: String,
    /// 事件发生时已捕获的安全操作人名称；未知保持缺失。
    pub actor_name_snapshot: Option<String>,
    /// 认证请求的安全关联标识；无请求时保持缺失。
    pub request_id: Option<String>,
    /// 同一业务命令内事件的正整数序号。
    pub event_sequence: NonZeroU32,
    /// Actor kind.
    pub actor_type: AccountKind,
    /// Business action name.
    pub action: String,
    /// Resource type.
    pub resource_type: String,
    /// Resource id.
    pub resource_id: Option<String>,
    /// Success flag; identity only prepares successful resource audits.
    pub success: bool,
    /// Optional business message.
    pub message: Option<String>,
}

impl_audit_context!(PreparedResourceAudit);

impl PreparedResourceAudit {
    /// 从已经校验的审计实体快照抄写身份侧字段。
    ///
    /// 名称快照和请求标识留空，事件序号取 `NonZeroU32::MIN`，由后续 `with_*` 补上。
    ///
    /// # 参数
    /// * `base` - 已构造审计的持久化元数据
    /// * `actor_id` - 操作人 ID
    /// * `actor_account` - 操作人登录账号
    /// * `actor_type` - 操作人种类
    /// * `action` - 动作名称
    /// * `resource_type` - 资源类型
    /// * `resource_id` - 资源 ID
    /// * `success` - 是否成功
    /// * `message` - 可选业务说明
    ///
    /// # 返回
    /// 返回稍后持久化的预制审计事实。
    ///
    /// # 错误
    /// 不返回错误；输入字段由调用方预先校验。
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

/// Port identity uses to prepare and persist resource audits on a caller executor.
#[async_trait]
pub trait IdentityAuditPort: Send + Sync {
    /// 在事务开始前校验并准备一条成功的资源审计。
    ///
    /// # 参数
    /// * `actor` - 已认证的操作人
    /// * `action` - 业务动作名称
    /// * `resource_type` - 资源类型
    /// * `resource_id` - 资源 ID
    ///
    /// # 返回
    /// 返回可在调用方执行器上持久化的预制审计事实。
    ///
    /// # 错误
    /// 操作人或资源字段不满足审计约束时返回错误。
    fn resource_log(
        &self,
        actor: AuditActor,
        action: &str,
        resource_type: &str,
        resource_id: String,
    ) -> Result<PreparedResourceAudit>;

    /// 把事先准备好的审计写到调用方选定的执行器。
    ///
    /// # 参数
    /// * `audit` - 已准备的成功资源审计
    /// * `executor` - 调用方选定的执行器
    ///
    /// # 返回
    /// 写入完成时无额外返回值。
    ///
    /// # 错误
    /// 持久化失败时返回错误。
    async fn persist(&self, audit: &PreparedResourceAudit, executor: &mut dyn Executor) -> Result<()>;
}
