//! Extension traits for collection repositories.

pub use super::access_control::{
    AuditEventRepositoryExt, DataScopeRepositoryExt, PermissionRepositoryExt, UserRoleRepositoryExt,
};
pub use super::account_core::AccountCoreRepositoryExt;
pub use super::role::RoleRepositoryExt;
