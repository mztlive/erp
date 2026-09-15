//! Identity domain: accounts, IAM/RBAC, Casbin and access control.

pub mod dto;
pub mod entity;
mod error;
pub mod indexes;
pub mod ports;
pub mod repository;
pub mod service;

pub use dto::*;
pub use entity::{
    AccountCore, AccountCoreData, AccountCoreUpdate, AccountStatus, LoginAccount, PasswordVerification,
    Permission, PermissionSet, Role, RoleData, RoleId, RoleIdSet, RoleUpdate, Secret, access_control,
};
pub use error::{Error, Result};
pub use ports::{AuthorizationPort, IdentityAuditPort, OrganizationScopeFact, PreparedResourceAudit};
pub use repository::{
    AccessControlExt, AccessControlRepository, AccountCoreRepository, AuditEventFilter, AuditEventRepository,
    AuditEventRow, CASBIN_RULES, DataScopeFilter, DataScopeRepository, DataScopeRow, MongoCasbinAdapter,
    PermissionFilter, PermissionRepository, PermissionRow, RoleRepository, UserRoleRepository,
};
pub use service::access_control::AccessControlService;
pub use service::auth::{AuthRequest, AuthResponse, BackofficeAuthResult, BackofficeAuthService};
pub use service::iam::{
    AccountProfile, AccountProfileService, AdminItem, AdminService, AuthorizedAccountManagement,
    AuthorizedRoleGrant, CreateAdminParams, CreateRoleParams, InitializeSuperAdminParams,
    InitializeSuperAdminResult, ROOT_ROLE_ID, RbacService, ResetAdminPasswordParams,
    ResetAdminPasswordResult, RoleItem, RolePermissionSnapshot, SharedRbacService, UpdateAdminParams,
    UpdateAdminRoleParams, UpdateRoleParams, ensure_predefined_roles, ensure_root_role, shared_rbac_service,
    subject,
};
