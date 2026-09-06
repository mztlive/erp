//! Composition-root authorization facts. Does not expose RbacService, AccountCore, or Role.

use async_trait::async_trait;

use crate::entity::access_control::OrganizationCoverage;
use crate::entity::rbac::Permission;
use crate::error::Result;

/// Minimal organization-scope fact returned to callers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrganizationScopeFact {
    coverage: OrganizationCoverage,
}

impl OrganizationScopeFact {
    /// Wrap an organization coverage fact.
    ///
    /// # Parameters
    /// * `coverage` - computed coverage
    ///
    /// # Returns
    /// Fact wrapper that does not expose Role or AccountCore.
    pub fn new(coverage: OrganizationCoverage) -> Self {
        Self { coverage }
    }

    /// Borrow the coverage fact.
    pub fn coverage(&self) -> &OrganizationCoverage {
        &self.coverage
    }
}

/// Authorization facts for composition-root adapters.
#[async_trait]
pub trait AuthorizationPort: Send + Sync {
    /// Return whether `subject` is allowed `permission`.
    ///
    /// # Parameters
    /// * `subject` - Casbin subject key
    /// * `permission` - required permission
    ///
    /// # Returns
    /// `true` when the subject is allowed.
    ///
    /// # Errors
    /// Policy load or enforcement failures.
    async fn allows(&self, subject: &str, permission: &Permission) -> Result<bool>;

    /// Return the subject's organization coverage fact, not a Role aggregate.
    ///
    /// # Parameters
    /// * `subject` - Casbin subject key
    ///
    /// # Returns
    /// Organization coverage when configured.
    ///
    /// # Errors
    /// Policy or data-scope query failures.
    async fn organization_scope(&self, subject: &str) -> Result<Option<OrganizationScopeFact>>;
}
