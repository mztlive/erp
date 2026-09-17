//! Consumer port for cross-domain audit persistence from warehouse commands.

use application_core::AuditActor;
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

impl PreparedWarehouseAudit {
    /// Capture warehouse-side fields from an already-validated audit entity snapshot.
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

    /// Capture warehouse-side fields from an audit facts struct.
    ///
    /// # 参数
    /// * `facts` - 审计事实参数结构体
    ///
    /// # 返回
    /// 返回稍后持久化的不透明审计事实。
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
            action: facts.action.clone(),
            resource_type: facts.resource_type.clone(),
            resource_id: facts.resource_id.clone(),
            success: facts.success,
            message: facts.message.clone(),
        }
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
        Self::resource_with_message(actor, action, resource_type, resource_id, None)
    }

    /// Build a success resource audit with an optional business message.
    ///
    /// # Errors
    /// Empty resource id.
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
        let (actor_id, actor_account, actor_type) = actor.into_parts();
        let id = id_generator::next_id();
        let base = BaseModel::new(id);
        Ok(Self::from_facts(&WarehouseAuditFacts {
            base,
            actor_id,
            actor_account,
            actor_type,
            action: action.to_string(),
            resource_type: resource_type.to_string(),
            resource_id: Some(resource_id),
            success: true,
            message,
        }))
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
    /// Validate and prepare a success resource audit before the transaction.
    fn resource_log(
        &self,
        actor: AuditActor,
        action: &str,
        resource_type: &str,
        resource_id: String,
    ) -> Result<PreparedWarehouseAudit>;

    /// Validate and prepare a success resource audit with a business message.
    fn resource_log_with_message(
        &self,
        actor: AuditActor,
        action: &str,
        resource_type: &str,
        resource_id: String,
        message: Option<String>,
    ) -> Result<PreparedWarehouseAudit>;

    /// Persist a previously prepared audit on the caller-chosen executor.
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
