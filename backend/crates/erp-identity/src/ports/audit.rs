//! Consumer port for cross-domain audit persistence from identity commands.

use application_core::AuditActor;
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

impl PreparedResourceAudit {
    /// Capture identity-side fields from an already-validated audit entity snapshot.
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
    /// Validate and prepare a success resource audit before the transaction.
    fn resource_log(
        &self,
        actor: AuditActor,
        action: &str,
        resource_type: &str,
        resource_id: String,
    ) -> Result<PreparedResourceAudit>;

    /// Persist a previously prepared audit on the caller-chosen executor.
    async fn persist(&self, audit: &PreparedResourceAudit, executor: &mut dyn Executor) -> Result<()>;
}
