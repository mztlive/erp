//! Object-read decision port for approval binding/revalidation.

use crate::entity::document_registry::DocumentType;
use crate::error::Result;

/// Domain object-read decisions used by approval binding.
pub trait ApprovalObjectReadPort: Send + Sync {
    /// Return whether `assignee_user_id` can read the subject.
    ///
    /// # Parameters
    /// * `document_type` - frozen document type
    /// * `organization_id` - document responsible organization
    /// * `creator_id` - document creator
    /// * `assignee_user_id` - candidate approver
    ///
    /// # Returns
    /// `Some(true/false)` when the domain adapter is wired; `None` when not wired.
    ///
    /// # Errors
    /// Empty organization or assignee, or domain adapter failures.
    fn object_read_decision(
        &self,
        document_type: DocumentType,
        organization_id: &str,
        creator_id: &str,
        assignee_user_id: &str,
    ) -> Result<Option<bool>>;
}

/// Fail-closed object-read port used when composition has not injected a domain adapter.
#[derive(Debug, Default, Clone, Copy)]
pub struct FailClosedObjectReadPort;

impl ApprovalObjectReadPort for FailClosedObjectReadPort {
    fn object_read_decision(
        &self,
        _document_type: DocumentType,
        organization_id: &str,
        _creator_id: &str,
        assignee_user_id: &str,
    ) -> Result<Option<bool>> {
        if organization_id.trim().is_empty() || assignee_user_id.trim().is_empty() {
            return Err(crate::error::Error::ValidationError(
                "单据组织或审批人不能为空".to_string(),
            ));
        }
        Ok(None)
    }
}
