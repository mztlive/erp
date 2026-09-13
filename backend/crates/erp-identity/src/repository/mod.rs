//! Identity MongoDB repositories, Casbin adapter and accessors.

pub mod access_control;
mod account_core;
pub mod casbin_adapter;
pub mod extensions;
pub mod organization;
pub mod owned;
mod role;
pub use organization::OrganizationRepository;

pub use access_control::{
    AccessControlRepository, AuditEventFilter, AuditEventRow, DataScopeFilter, DataScopeRow,
    PermissionFilter, PermissionRow,
};
pub use casbin_adapter::{MongoCasbinAdapter, CASBIN_RULES};
pub use extensions::AccessControlExt;
pub use owned::{
    AccountCoreRepository, AuditEventRepository, DataScopeRepository, PermissionRepository, RoleRepository,
    UserRoleRepository,
};
