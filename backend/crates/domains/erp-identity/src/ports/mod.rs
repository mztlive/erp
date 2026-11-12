//! 身份领域供消费方与组合根使用的端口。

mod access;
mod audit;
pub use access::{AccessFactsPort, AccessMapping, MappingPreparation};
mod organization;
pub use audit::{IdentityAuditPort, PreparedResourceAudit};
pub use organization::OrganizationBusinessPort;

mod scope_targets;
pub use scope_targets::ScopeTargetPort;

mod resource_grants;
pub use resource_grants::ResourceGrantFacts;

mod query;
pub use query::{QueryFragment, RelationMapping};
