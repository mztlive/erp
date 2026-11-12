//! 身份领域：账号、IAM/RBAC、Casbin 与访问控制。

pub mod dto;
pub mod entity;
mod error;
pub mod indexes;
pub mod ports;
pub mod repository;
pub mod service;

pub use dto::*;
pub use entity::authorization::{AccessTarget, ResourceTargets};
pub use entity::portal::{PortalActor, PortalBinding, PortalRole};
pub use entity::{
    AccountCore, AccountCoreData, AccountCoreUpdate, AccountStatus, LoginAccount, PasswordVerification,
    Permission, PermissionSet, Role, RoleData, RoleId, RoleIdSet, RoleUpdate, Secret, access_control,
};
pub use error::{Error, Result};
pub use ports::{AccessFactsPort, AccessMapping, IdentityAuditPort, PreparedResourceAudit};
pub use repository::authorization::CompiledAccess;
pub use repository::portal::{PortalBindingRepository, PortalBindingRepositoryExt, PortalIdentityExt};
pub use repository::{
    AccessControlExt, AccountCoreRepository, AccountCoreRepositoryExt, AuditEventFilter,
    AuditEventRepository, AuditEventRepositoryExt, AuditEventRow, CASBIN_RULES, MongoCasbinAdapter,
    PermissionFilter, PermissionRepository, PermissionRepositoryExt, RoleRepository, RoleRepositoryExt,
    UserRoleRepository, UserRoleRepositoryExt,
};
pub use service::access_control::AccessControlService;
pub use service::auth::{AuthRequest, AuthResponse, BackofficeAuthResult, BackofficeAuthService};
pub use service::authorization::{AuthorizationBuilder, AuthorizationService};
pub use service::directory::AccountDirectoryService;
pub use service::iam::{
    AccountProfile, AccountProfileService, AdminItem, AdminService, AuthorizedAccountManagement,
    AuthorizedRoleGrant, CreateAdminParams, CreateRoleParams, InitializeSuperAdminParams,
    InitializeSuperAdminResult, QualifiedPermissions, ROOT_ROLE_ID, RbacService, ResetAdminPasswordParams,
    ResetAdminPasswordResult, RoleItem, RolePermissionSnapshot, SharedRbacService, UpdateAdminParams,
    UpdateAdminRoleParams, UpdateRoleParams, ensure_root_role, shared_rbac_service, subject,
};
pub use service::organization_directory::OrganizationDirectoryService;
pub use service::portal::{PortalIdentityService, PreparedPortalAccount, PreparedPortalPassword};
