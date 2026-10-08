//! 集合仓储的扩展 trait。

pub use super::access_control::{
    AuditEventRepositoryExt, DataScopeRepositoryExt, PermissionRepositoryExt, UserRoleRepositoryExt,
};
pub use super::account_core::AccountCoreRepositoryExt;
pub use super::person_directory::PersonQueryQualificationRepositoryExt;
pub use super::role::RoleRepositoryExt;
