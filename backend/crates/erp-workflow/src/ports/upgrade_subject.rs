//! Strong-subject facts for approval binding upgrade.

use async_trait::async_trait;
use persistence_core::Executor;

use crate::entity::document_registry::DocumentType;
use crate::error::Result;

/// Approval binding upgrade facts from the strong business object.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApprovalUpgradeSubjectFacts {
    /// Request type after business-nature verification.
    pub document_type: DocumentType,
    /// Strong business-object id.
    pub document_id: String,
    /// Strong business-object `BaseModel.version`.
    pub business_object_version: u64,
    /// Formal document no when assigned.
    pub document_no: String,
    /// Responsible organization from the object or parent chain.
    pub responsible_org_id: String,
    /// Immutable creator.
    pub creator_id: String,
}

impl ApprovalUpgradeSubjectFacts {
    /// Reject a stale client-submitted business-object version.
    pub fn ensure_expected_business_object_version(&self, expected: u64) -> crate::error::Result<()> {
        if self.business_object_version != expected {
            return Err(crate::error::Error::ConflictError("业务对象版本已变化，请刷新后重试".to_string()));
        }
        Ok(())
    }
}

/// Loads upgrade facts from owning business domains.
#[async_trait]
pub trait UpgradeSubjectPort: Send + Sync {
    /// Load strong-subject facts for a process-required document.
    ///
    /// # Parameters
    /// * `document_type` - exact document type
    /// * `document_id` - exact business-object id
    /// * `executor` - caller executor
    ///
    /// # Returns
    /// Identity, version, document no, responsible org and creator.
    ///
    /// # Errors
    /// Missing entity, type mismatch, or incomplete creator/org facts.
    async fn load(
        &self,
        document_type: DocumentType,
        document_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<ApprovalUpgradeSubjectFacts>;

    /// Enforce Fresh-only unsubmitted gates on already loaded facts.
    async fn ensure_initial_unsubmitted(
        &self,
        facts: &ApprovalUpgradeSubjectFacts,
        executor: &mut dyn Executor,
    ) -> Result<()>;
}

/// Fail-closed upgrade port used when composition has not injected a domain adapter.
#[derive(Debug, Default, Clone, Copy)]
pub struct FailClosedUpgradeSubjectPort;

#[async_trait]
impl UpgradeSubjectPort for FailClosedUpgradeSubjectPort {
    async fn load(
        &self,
        _document_type: DocumentType,
        _document_id: &str,
        _executor: &mut dyn Executor,
    ) -> Result<ApprovalUpgradeSubjectFacts> {
        Err(crate::error::Error::ValidationError("审批升级对象读取未接线，已按安全策略拒绝".to_string()))
    }

    async fn ensure_initial_unsubmitted(
        &self,
        _facts: &ApprovalUpgradeSubjectFacts,
        _executor: &mut dyn Executor,
    ) -> Result<()> {
        Err(crate::error::Error::ValidationError("审批升级对象读取未接线，已按安全策略拒绝".to_string()))
    }
}
