//! Owned identity repositories composed from persistence-core.

mod account_core;
mod audit_event;
mod data_scope;
mod permission;
mod role;
mod user_role;

pub use account_core::AccountCoreRepository;
pub use audit_event::AuditEventRepository;
pub use data_scope::DataScopeRepository;
pub use permission::PermissionRepository;
pub use role::RoleRepository;
pub use user_role::UserRoleRepository;
