//! Identity MongoDB repositories, Casbin adapter and accessors.

pub mod access_control;
mod account_core;
pub mod casbin_adapter;
pub mod extensions;
pub mod organization;
pub mod owned;
pub mod prelude;
mod role;
pub use access_control::{
    AccessControlRepository, AuditEventFilter, AuditEventRepositoryExt, AuditEventRow, DataScopeFilter,
    DataScopeRepositoryExt, DataScopeRow, PermissionFilter, PermissionRepositoryExt, PermissionRow,
    UserRoleRepositoryExt,
};
pub use account_core::AccountCoreRepositoryExt;
pub use casbin_adapter::{CASBIN_RULES, MongoCasbinAdapter};
pub use extensions::AccessControlExt;
pub use organization::OrganizationRepository;
pub use owned::{
    AccountCoreRepository, AuditEventRepository, DataScopeRepository, PermissionRepository, RoleRepository,
    UserRoleRepository,
};
pub use role::RoleRepositoryExt;
