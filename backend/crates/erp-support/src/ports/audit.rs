//! Consumer port for cross-domain audit persistence from support commands.

use std::num::NonZeroU32;

use application_core::AuditActor;
use async_trait::async_trait;
use entity_core::BaseModel;
use erp_core::AccountKind;
use persistence_core::Executor;

use crate::error::{Error, Result};

/// Prepared successful resource audit that support can persist through a port.
///
/// Support never depends on `erp-audit` types. Composition-root adapters convert
/// this fact into an `AuditLog` and write it on the same [`Executor`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedSupportAudit {
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
    /// Success flag; support only prepares successful resource audits.
    pub success: bool,
    /// Optional business message.
    pub message: Option<String>,
}

impl PreparedSupportAudit {
    /// Capture support-side fields from an already-validated audit entity snapshot.
    ///
    /// # Parameters
    /// * `base` - persistence metadata of the constructed audit
    /// * `actor_id` - actor id
    /// * `actor_account` - actor login
    /// * `actor_type` - actor kind
    /// * `action` - action name
    /// * `resource_type` - resource type
    /// * `resource_id` - resource id
    /// * `success` - success flag
    /// * `message` - optional message
    ///
    /// # Returns
    /// Opaque prepared audit facts for later persistence.
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

    /// Carry an already validated safe actor name without reading current account data.
    ///
    /// # 参数
    /// * `name` - 认证时已经校验的安全名称快照；缺失时保持 `None`。
    ///
    /// # 返回
    /// 返回保留原持久化元数据并携带名称快照的审计事实。
    ///
    /// # 错误
    /// 无；组合层在构造结构化事件时再次校验该快照。
    pub fn with_actor_name_snapshot(mut self, name: Option<String>) -> Self {
        self.actor_name_snapshot = name;
        self
    }

    /// Carry an already validated request correlation without generating one.
    ///
    /// # 参数
    /// * `request_id` - 调用时的安全请求编号；缺失时保持 `None`。
    ///
    /// # 返回
    /// 返回保留原持久化元数据并携带请求关联的审计事实。
    ///
    /// # 错误
    /// 无；组合层在构造结构化事件时再次校验编号。
    pub fn with_request_id(mut self, request_id: Option<String>) -> Self {
        self.request_id = request_id;
        self
    }

    /// Preserve the positive order of an already prepared event.
    ///
    /// # 参数
    /// * `event_sequence` - 命令内从一开始的非零事件序号。
    ///
    /// # 返回
    /// 返回保留原事件顺序的审计事实。
    ///
    /// # 错误
    /// 无；参数类型保证序号非零。
    pub fn with_event_sequence(mut self, event_sequence: NonZeroU32) -> Self {
        self.event_sequence = event_sequence;
        self
    }

    /// Build a success resource audit from an authenticated actor.
    ///
    /// # Errors
    /// Empty resource id.
    pub fn resource(
        actor: AuditActor,
        action: &str,
        resource_type: &str,
        resource_id: String,
    ) -> Result<Self> {
        if resource_id.trim().is_empty() {
            return Err(Error::ValidationError("资源ID不能为空".to_string()));
        }
        let actor_name_snapshot = actor.actor_name_snapshot().map(str::to_string);
        let request_id = actor.request_id().map(str::to_string);
        let (actor_id, actor_account, actor_type) = actor.into_parts();
        let id = id_generator::next_id();
        let base = BaseModel::new(id);
        Ok(Self::from_validated(
            &base,
            actor_id,
            actor_account,
            actor_type,
            action.to_string(),
            resource_type.to_string(),
            Some(resource_id),
            true,
            None,
        )
        .with_actor_name_snapshot(actor_name_snapshot)
        .with_request_id(request_id))
    }
}

/// Port support uses to prepare and persist resource audits on a caller executor.
#[async_trait]
pub trait SupportAuditPort: Send + Sync {
    /// Validate and prepare a success resource audit before the transaction.
    fn resource_log(
        &self,
        actor: AuditActor,
        action: &str,
        resource_type: &str,
        resource_id: String,
    ) -> Result<PreparedSupportAudit>;

    /// Persist a previously prepared audit on the caller-chosen executor.
    async fn persist(&self, audit: &PreparedSupportAudit, executor: &mut dyn Executor) -> Result<()>;
}

/// Fail-closed audit port used when composition has not injected an adapter.
#[derive(Debug, Default, Clone, Copy)]
pub struct FailClosedAuditPort;

#[async_trait]
impl SupportAuditPort for FailClosedAuditPort {
    fn resource_log(
        &self,
        actor: AuditActor,
        action: &str,
        resource_type: &str,
        resource_id: String,
    ) -> Result<PreparedSupportAudit> {
        PreparedSupportAudit::resource(actor, action, resource_type, resource_id)
    }

    async fn persist(&self, _audit: &PreparedSupportAudit, _executor: &mut dyn Executor) -> Result<()> {
        Err(Error::Internal("审计端口未接线".to_string()))
    }
}
