//! 身份领域供消费方与组合根使用的端口。

mod audit;
mod authorization;
mod organization;
pub use audit::{IdentityAuditPort, PreparedResourceAudit};
pub use authorization::{AuthorizationPort, OrganizationScopeFact};
pub use organization::OrganizationBusinessPort;

mod scope_targets;
pub use scope_targets::ScopeTargetPort;
