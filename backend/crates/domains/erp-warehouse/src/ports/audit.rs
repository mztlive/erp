//! 仓库命令跨域持久化审计的消费端口。

use std::num::NonZeroU32;

use application_core::{AuditActor, AuditContext, impl_audit_context};
use async_trait::async_trait;
use entity_core::BaseModel;
use erp_core::AccountKind;
use persistence_core::Executor;

use crate::error::{Error, Result};

/// Prepared successful resource audit that warehouse can persist through a port.
///
/// Warehouse never depends on `erp-audit` types. Composition-root adapters convert
/// this fact into an `AuditLog` and write it on the same [`Executor`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedWarehouseAudit {
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
    /// Success flag; warehouse only prepares successful resource audits.
    pub success: bool,
    /// Optional business message.
    pub message: Option<String>,
}

impl_audit_context!(PreparedWarehouseAudit);

impl PreparedWarehouseAudit {
    /// 从已验证日志复制持久化元数据与业务身份，形成稍后写入的仓库审计事实；名称、请求号保持 `None`，事件序号固定为 `1`。
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
        Self::from_facts(&WarehouseAuditFacts {
            base: base.clone(),
            actor_id,
            actor_account,
            actor_type,
            action,
            resource_type,
            resource_id,
            success,
            message,
        })
    }

    /// 将事实参数转换成待持久化审计，供组合层传递已准备字段；不补操作人名称和请求号，事件序号从 `1` 开始。
    pub fn from_facts(facts: &WarehouseAuditFacts) -> Self {
        Self {
            id: facts.base.id.clone(),
            version: facts.base.version,
            created_at: facts.base.created_at,
            updated_at: facts.base.updated_at,
            deleted_at: facts.base.deleted_at,
            actor_id: facts.actor_id.clone(),
            actor_account: facts.actor_account.clone(),
            actor_type: facts.actor_type,
            actor_name_snapshot: None,
            request_id: None,
            event_sequence: NonZeroU32::MIN,
            action: facts.action.clone(),
            resource_type: facts.resource_type.clone(),
            resource_id: facts.resource_id.clone(),
            success: facts.success,
            message: facts.message.clone(),
        }
    }

    /// 用当前认证操作人和目标资源构造无附加说明的成功审计，保留名称快照及请求号以供追溯；资源 ID 去空白后为空时返回 `ValidationError`。
    pub fn resource(
        actor: AuditActor,
        action: &str,
        resource_type: &str,
        resource_id: String,
    ) -> Result<Self> {
        Self::resource_with_message(actor, action, resource_type, resource_id, None)
    }

    /// 为成功资源操作准备带可选业务说明的审计，沿用当前操作人名称和请求号；`None` 表示不附说明，资源 ID 去空白后为空则返回 `ValidationError`。
    pub fn resource_with_message(
        actor: AuditActor,
        action: &str,
        resource_type: &str,
        resource_id: String,
        message: Option<String>,
    ) -> Result<Self> {
        if resource_id.trim().is_empty() {
            return Err(Error::ValidationError("资源ID不能为空".to_string()));
        }

        let context = AuditContext::from(actor);
        let id = id_generator::next_id();
        let base = BaseModel::new(id);
        Ok(Self::from_facts(&WarehouseAuditFacts {
            base,
            actor_id: context.actor_id,
            actor_account: context.actor_account,
            actor_type: context.actor_type,
            action: action.to_string(),
            resource_type: resource_type.to_string(),
            resource_id: Some(resource_id),
            success: true,
            message,
        })
        .with_actor_name_snapshot(context.actor_name_snapshot)
        .with_request_id(context.request_id))
    }
}

/// 审计事实参数结构体（承接 `from_validated` 的字段组，调用点按字段名赋值避免顺序传错）。
#[derive(Debug, Clone)]
pub struct WarehouseAuditFacts {
    /// 已构造审计的持久化元数据。
    pub base: BaseModel,
    /// 操作人 ID。
    pub actor_id: String,
    /// 操作人登录账号。
    pub actor_account: String,
    /// 操作人类型。
    pub actor_type: AccountKind,
    /// 业务动作名。
    pub action: String,
    /// 资源类型。
    pub resource_type: String,
    /// 资源 ID。
    pub resource_id: Option<String>,
    /// 成功标记；仓库只准备成功的资源审计。
    pub success: bool,
    /// 可选业务消息。
    pub message: Option<String>,
}

/// Port warehouse uses to prepare and persist resource audits on a caller executor.
#[async_trait]
pub trait WarehouseAuditPort: Send + Sync {
    /// 在持久化前准备已认证操作人、动作与资源的成功审计，供原业务执行器保存；输入校验或审计准备失败时返回错误。
    fn resource_log(
        &self,
        actor: AuditActor,
        action: &str,
        resource_type: &str,
        resource_id: String,
    ) -> Result<PreparedWarehouseAudit>;

    /// 校验并准备带可选说明的成功审计，供业务与审计在同一事务写入；无法校验或构造时返回错误。
    fn resource_log_with_message(
        &self,
        actor: AuditActor,
        action: &str,
        resource_type: &str,
        resource_id: String,
        message: Option<String>,
    ) -> Result<PreparedWarehouseAudit>;

    /// 用调用方的业务执行器保存已准备的审计，使业务写入和留痕一起提交；持久化失败直接返回错误。
    async fn persist(&self, audit: &PreparedWarehouseAudit, executor: &mut dyn Executor) -> Result<()>;
}

/// Fail-closed audit port used when composition has not injected an adapter.
#[derive(Debug, Default, Clone, Copy)]
pub struct FailClosedAuditPort;

#[async_trait]
impl WarehouseAuditPort for FailClosedAuditPort {
    fn resource_log(
        &self,
        actor: AuditActor,
        action: &str,
        resource_type: &str,
        resource_id: String,
    ) -> Result<PreparedWarehouseAudit> {
        PreparedWarehouseAudit::resource(actor, action, resource_type, resource_id)
    }

    fn resource_log_with_message(
        &self,
        actor: AuditActor,
        action: &str,
        resource_type: &str,
        resource_id: String,
        message: Option<String>,
    ) -> Result<PreparedWarehouseAudit> {
        PreparedWarehouseAudit::resource_with_message(actor, action, resource_type, resource_id, message)
    }

    async fn persist(&self, _audit: &PreparedWarehouseAudit, _executor: &mut dyn Executor) -> Result<()> {
        Err(Error::Internal("审计端口未接线".to_string()))
    }
}
