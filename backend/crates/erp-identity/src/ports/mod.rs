//! Identity consumer and composition-root ports.

mod audit;
mod authorization;
mod organization;
pub use organization::OrganizationBusinessPort;

pub use audit::{IdentityAuditPort, PreparedResourceAudit};
pub use authorization::{AuthorizationPort, OrganizationScopeFact};

mod scope_targets;
pub use scope_targets::ScopeTargetPort;
