//! 合同命令跨域持久化审计的消费方端口。

use std::num::NonZeroU32;

use application_core::{AuditActor, AuditContext, impl_audit_context};
use async_trait::async_trait;
use entity_core::BaseModel;
use erp_core::AccountKind;
use persistence_core::Executor;

use crate::error::{Error, Result};

/// Prepared successful resource audit that contract can persist through a port.
///
/// Contract never depends on `erp-audit` types. Composition-root adapters convert
/// this fact into an `AuditLog` and write it on the same [`Executor`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedContractAudit {
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
    /// Actor kind.
    pub actor_type: AccountKind,
    /// Safe actor display-name snapshot captured when the command was authenticated.
    pub actor_name_snapshot: Option<String>,
    /// Request correlation captured at invocation; absent outside a request.
    pub request_id: Option<String>,
    /// Positive event order within one business command.
    pub event_sequence: NonZeroU32,
    /// Business action name.
    pub action: String,
    /// Resource type.
    pub resource_type: String,
    /// Resource id.
    pub resource_id: Option<String>,
    /// Success flag; contract only prepares successful resource audits.
    pub success: bool,
    /// Optional business message.
    pub message: Option<String>,
}

impl_audit_context!(PreparedContractAudit);

impl PreparedContractAudit {
    /// 从已校验的审计实体快照摘取合同侧字段。
    /// 名称快照与请求编号留空，事件序号为 1；二者由后续方法填入。
    /// 供稍后持久化的审计事实。
    /// `base` 为已构造审计的持久化元数据；`actor_id` 为操作人 ID。
    /// `actor_account` 为登录账号；`actor_type` 为操作人种类。
    /// `action` 为动作名；`resource_type` 为资源类型。
    /// `resource_id` 为资源 ID；`success` 为是否成功。
    /// `message` 为可选说明。
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
            actor_type,
            actor_name_snapshot: None,
            request_id: None,
            event_sequence: NonZeroU32::MIN,
            action,
            resource_type,
            resource_id,
            success,
            message,
        }
    }

    /// 由已认证操作人构建成功的资源审计。
    /// 成功标志为真、说明为空的待持久化审计事实。事件序号为 1。
    /// `actor` 为已认证操作人；名称快照与请求编号取自该值；`action` 为业务动作名。
    /// `resource_type` 为资源类型；`resource_id` 为资源 ID。
    /// `resource_id` 去空白后为空时返回 `ValidationError`。
    pub fn resource(
        actor: AuditActor,
        action: &str,
        resource_type: &str,
        resource_id: String,
    ) -> Result<Self> {
        if resource_id.trim().is_empty() {
            return Err(Error::ValidationError("资源ID不能为空".to_string()));
        }
        let context = AuditContext::from(actor);
        let id = id_generator::next_id();
        let base = BaseModel::new(id);
        Ok(Self::from_validated(
            &base,
            context.actor_id,
            context.actor_account,
            context.actor_type,
            action.to_string(),
            resource_type.to_string(),
            Some(resource_id),
            true,
            None,
        )
        .with_actor_name_snapshot(context.actor_name_snapshot)
        .with_request_id(context.request_id))
    }
}

/// Port contract uses to prepare and persist resource audits on a caller executor.
#[async_trait]
pub trait ContractAuditPort: Send + Sync {
    /// 在事务开始前校验并准备一条成功的资源审计。
    /// 待在同一执行器上持久化的审计事实。
    /// `action` 为业务动作名，`resource_type` 为资源类型，`resource_id` 为资源 ID。
    /// 资源 ID 为空时返回错误。
    fn resource_log(
        &self,
        actor: AuditActor,
        action: &str,
        resource_type: &str,
        resource_id: String,
    ) -> Result<PreparedContractAudit>;

    /// 在调用方选定的执行器上持久化先前准备好的审计。
    /// 无返回值。持久化完成。
    /// `audit` 为已准备的审计事实；`executor` 为调用方选定的执行器。
    /// 适配器写入失败时返回对应错误。
    async fn persist(&self, audit: &PreparedContractAudit, executor: &mut dyn Executor) -> Result<()>;
}

/// Fail-closed audit port used when composition has not injected an adapter.
#[derive(Debug, Default, Clone, Copy)]
pub struct FailClosedAuditPort;

#[async_trait]
impl ContractAuditPort for FailClosedAuditPort {
    fn resource_log(
        &self,
        actor: AuditActor,
        action: &str,
        resource_type: &str,
        resource_id: String,
    ) -> Result<PreparedContractAudit> {
        PreparedContractAudit::resource(actor, action, resource_type, resource_id)
    }

    async fn persist(&self, _audit: &PreparedContractAudit, _executor: &mut dyn Executor) -> Result<()> {
        Err(Error::Internal("审计端口未接线".to_string()))
    }
}
