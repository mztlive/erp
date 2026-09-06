//! Audit persistence consumed by workflow; adapters live at the composition root.

use async_trait::async_trait;
use persistence_core::Executor;

use application_core::{AuditActor, CommandReceipt, CommandReceiptFact};

use crate::error::{Error, Result};

/// Read-side audit fact consumed by workflow without depending on the audit domain.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkflowAuditFact {
    /// Actor account id.
    pub actor_id: String,
    /// Business action.
    pub action: String,
    /// Resource type.
    pub resource_type: String,
    /// Resource id when present.
    pub resource_id: Option<String>,
    /// Whether the recorded action succeeded.
    pub success: bool,
    /// Optional message, including command fingerprints.
    pub message: Option<String>,
}

impl WorkflowAuditFact {
    /// Construct a successful resource audit fact for tests and adapters.
    pub fn successful(
        actor_id: impl Into<String>,
        action: impl Into<String>,
        resource_type: impl Into<String>,
        resource_id: impl Into<String>,
    ) -> Self {
        Self {
            actor_id: actor_id.into(),
            action: action.into(),
            resource_type: resource_type.into(),
            resource_id: Some(resource_id.into()),
            success: true,
            message: None,
        }
    }
}

/// Prepared success audit that workflow can persist through [`WorkflowAuditPort`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedWorkflowAudit {
    /// Stable audit id.
    pub id: String,
    /// Actor account id.
    pub actor_id: String,
    /// Actor login account.
    pub actor_account: String,
    /// Actor kind wire value.
    pub actor_type: String,
    /// Business action.
    pub action: String,
    /// Resource type.
    pub resource_type: String,
    /// Resource id.
    pub resource_id: String,
    /// Optional message, including command fingerprints.
    pub message: Option<String>,
}

impl PreparedWorkflowAudit {
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

    /// Build a success resource audit with an optional message.
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
        Self::resource_with_id(
            id_generator::next_id(),
            actor,
            action,
            resource_type,
            resource_id,
            message,
        )
    }

    /// Build a success resource audit with an explicit id and optional message.
    ///
    /// # Errors
    /// Empty resource id.
    pub fn resource_with_id(
        id: String,
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
        Ok(Self {
            id,
            actor_id,
            actor_account,
            actor_type: actor_type.as_str().to_string(),
            action: action.to_string(),
            resource_type: resource_type.to_string(),
            resource_id,
            message,
        })
    }

    /// Build the success receipt audit that must share the business write transaction.
    ///
    /// # Errors
    /// Actor mismatch or empty resource id.
    pub fn from_receipt(receipt: &CommandReceipt, actor: AuditActor, resource_id: String) -> Result<Self> {
        if actor.id() != receipt.actor_id() {
            return Err(Error::Forbidden("当前账号不能复用其他账号的操作号".to_string()));
        }
        Self::resource_with_id(
            receipt.id().to_string(),
            actor,
            receipt.action(),
            receipt.resource_type(),
            resource_id,
            Some(receipt.message(None)),
        )
    }
}

/// Persist and query workflow audits without depending on the audit domain crate.
#[async_trait]
pub trait WorkflowAuditPort: Send + Sync {
    /// Persist a prepared success audit using the caller executor.
    async fn persist(&self, audit: &PreparedWorkflowAudit, executor: &mut dyn Executor) -> Result<()>;

    /// Load command-receipt facts for the given ids.
    async fn find_command_receipts_by_ids(
        &self,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<CommandReceiptFact>>;

    /// Load successful audits for one resource without exposing the audit aggregate.
    async fn list_successful_resource_audits(
        &self,
        resource_type: &str,
        resource_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Vec<WorkflowAuditFact>>;
}

/// Fail-closed audit port used when composition has not injected an adapter.
#[derive(Debug, Default, Clone, Copy)]
pub struct FailClosedAuditPort;

#[async_trait]
impl WorkflowAuditPort for FailClosedAuditPort {
    async fn persist(&self, _audit: &PreparedWorkflowAudit, _executor: &mut dyn Executor) -> Result<()> {
        Err(Error::Internal("审计端口未接线".to_string()))
    }

    async fn find_command_receipts_by_ids(
        &self,
        _ids: &[String],
        _executor: &mut dyn Executor,
    ) -> Result<Vec<CommandReceiptFact>> {
        Ok(Vec::new())
    }

    async fn list_successful_resource_audits(
        &self,
        _resource_type: &str,
        _resource_id: &str,
        _executor: &mut dyn Executor,
    ) -> Result<Vec<WorkflowAuditFact>> {
        Ok(Vec::new())
    }
}

/// Resolve a committed command receipt without holding a workflow session over I/O.
pub async fn committed_resource_id(
    port: &dyn WorkflowAuditPort,
    receipt: &CommandReceipt,
) -> Result<Option<String>> {
    let candidates = receipt.id_candidates();
    let facts = port
        .find_command_receipts_by_ids(&candidates, &mut persistence_core::NoTransaction)
        .await?;
    for candidate in candidates {
        let Some(fact) = facts.iter().find(|fact| fact.id == candidate) else {
            continue;
        };
        return match receipt.match_fact(fact) {
            application_core::CommandReceiptMatch::SamePayload(resource_id) => Ok(Some(resource_id)),
            application_core::CommandReceiptMatch::DifferentPayload => Err(Error::ConflictError(
                "同一操作号已用于不同提交，请重新发起操作".to_string(),
            )),
            application_core::CommandReceiptMatch::Corrupted => {
                Err(Error::Internal("业务命令收据格式无效".to_string()))
            }
        };
    }
    Ok(None)
}
