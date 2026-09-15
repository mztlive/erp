//! Upgrade-subject loading is owned by [`crate::ports::UpgradeSubjectPort`] adapters.

use persistence_core::Executor;

use crate::entity::document_registry::DocumentType;
use crate::error::Result;
pub use crate::ports::ApprovalUpgradeSubjectFacts;
use crate::ports::UpgradeSubjectPort;

/// Load strong-subject facts through the injected port.
pub async fn load_approval_upgrade_subject_facts(
    port: &dyn UpgradeSubjectPort,
    document_type: DocumentType,
    document_id: &str,
    executor: &mut dyn Executor,
) -> Result<ApprovalUpgradeSubjectFacts> {
    port.load(document_type, document_id, executor).await
}

/// Enforce Fresh-only unsubmitted gates through the injected port.
pub async fn ensure_initial_unsubmitted_approval_upgrade_subject(
    port: &dyn UpgradeSubjectPort,
    facts: &ApprovalUpgradeSubjectFacts,
    executor: &mut dyn Executor,
) -> Result<()> {
    port.ensure_initial_unsubmitted(facts, executor).await
}

impl ApprovalUpgradeSubjectFacts {
    /// Binding revalidation context from strong-subject facts.
    pub fn binding_context(&self) -> crate::service::approval::business_adapter::BindingRevalidationContext {
        crate::service::approval::business_adapter::BindingRevalidationContext {
            order_source: None,
            customer_id: None,
            business_org_unit_id: None,
            scope_owner_user_id: None,
            organization_id: self.responsible_org_id.clone(),
            creator_id: self.creator_id.clone(),
        }
    }
}
