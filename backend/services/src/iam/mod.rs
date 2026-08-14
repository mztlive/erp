/// IAM 模块。
///
/// 该模块承载账号服务以及 Casbin RBAC 边界。
mod account;
mod dto;
mod predefined_data_scopes;
mod predefined_roles;
mod rbac;

pub use account::{
    AccountProfile, AccountProfileService, AdminItem, AdminService, CreateAdminParams,
    InitializeSuperAdminParams, InitializeSuperAdminResult, UpdateAdminParams, UpdateAdminRoleParams,
};
pub use dto::{CreateRoleParams, RoleItem, UpdateRoleParams};
pub use predefined_roles::ensure_predefined_roles;
pub use rbac::{ensure_root_role, shared_rbac_service, subject, RbacService, SharedRbacService};
pub(crate) use rbac::{AuthorizedAccountManagement, AuthorizedRoleGrant, ROOT_ROLE_ID};
