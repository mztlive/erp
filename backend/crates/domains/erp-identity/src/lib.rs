//! 身份与组织：登录、门户、组织与人员目录、账号核心。
//! 授权引擎与治理：IAM、数据范围、业务授权、记录授权与授权文件。

pub mod dto;
pub mod entity;
mod error;
pub mod indexes;
pub mod ports;
pub mod repository;
pub mod service;

pub use dto::*;
pub use error::{Error, Result};
pub use ports::{IdentityAuditPort, PreparedResourceAudit};
// 身份与组织：认证、门户、账号核心及目录。
pub use {
    entity::{
        AccountCore, AccountCoreData, AccountCoreUpdate, AccountStatus, LoginAccount, PasswordVerification,
        Secret,
        portal::{PortalActor, PortalBinding, PortalRole},
    },
    repository::{
        AccountCoreRepository, AccountCoreRepositoryExt,
        portal::{PortalBindingRepository, PortalBindingRepositoryExt, PortalIdentityExt},
    },
    service::{
        auth::{AuthRequest, AuthResponse, BackofficeAuthResult, BackofficeAuthService},
        directory::AccountDirectoryService,
        organization_directory::OrganizationDirectoryService,
        portal::{PortalIdentityService, PreparedPortalAccount, PreparedPortalPassword},
    },
};
// 授权引擎与治理：记录授权和授权文件保留各自模块入口。
pub use {
    entity::{
        Permission, PermissionSet, Role, RoleData, RoleId, RoleIdSet, RoleUpdate, access_control,
        authorization::{AccessTarget, ResourceTargets},
    },
    ports::{AccessFactsPort, AccessMapping},
    repository::{
        AccessControlExt, AuditEventFilter, AuditEventRepository, AuditEventRepositoryExt, AuditEventRow,
        CASBIN_RULES, MongoCasbinAdapter, PermissionFilter, PermissionRepository, PermissionRepositoryExt,
        RoleRepository, RoleRepositoryExt, UserRoleRepository, UserRoleRepositoryExt,
        authorization::CompiledAccess,
    },
    service::{
        access_control::AccessControlService,
        authorization::{AuthorizationBuilder, AuthorizationService},
        iam::{
            AccountProfile, AccountProfileService, AdminItem, AdminService, AuthorizedAccountManagement,
            AuthorizedRoleGrant, CreateAdminParams, CreateRoleParams, InitializeSuperAdminParams,
            InitializeSuperAdminResult, QualifiedPermissions, ROOT_ROLE_ID, RbacService,
            ResetAdminPasswordParams, ResetAdminPasswordResult, RoleItem, RolePermissionSnapshot,
            SharedRbacService, UpdateAdminParams, UpdateAdminRoleParams, UpdateRoleParams, ensure_root_role,
            shared_rbac_service, subject,
        },
    },
};

#[cfg(feature = "test-support")]
pub mod testing;
