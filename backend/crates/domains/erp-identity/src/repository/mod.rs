//! 身份领域的 MongoDB 仓储、策略存储与集合访问器。

pub mod access_control;
mod account_core;
pub mod authorization;
pub(crate) mod database_reset;
pub mod extensions;
pub mod organization;
pub mod owned;
mod person_directory;
pub(crate) mod person_directory_query;
pub(crate) mod policy_receipt;
pub mod policy_store;
pub mod portal;
pub mod prelude;
mod role;
pub use access_control::{AuditEventFilter, AuditEventRepositoryExt, AuditEventRow};
pub use account_core::AccountCoreRepositoryExt;
pub use extensions::AccessControlExt;
pub use organization::OrganizationRepository;
pub use owned::{
    AccountCoreRepository, AuditEventRepository, PersonQueryQualificationRepository, RoleRepository,
};
pub use person_directory::PersonQueryQualificationRepositoryExt;
pub use policy_store::{CASBIN_RULES, PolicyStore};
pub use role::RoleRepositoryExt;

pub mod resource_grants;

pub(crate) mod fingerprint;
