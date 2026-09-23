//! Collection-scoped aliases for [`persistence_core::Repository`].
//!
//! Domain-specific methods are extension traits on the generic repository.

pub type AccountCoreRepository<'a> = persistence_core::Repository<'a, crate::entity::AccountCore>;
pub type PersonQueryQualificationRepository<'a> =
    persistence_core::Repository<'a, crate::entity::person_directory::PersonQueryQualification>;
pub type AuditEventRepository<'a> =
    persistence_core::Repository<'a, crate::entity::access_control::AuditEvent>;
pub type DataScopeRepository<'a> = persistence_core::Repository<'a, crate::entity::access_control::DataScope>;
pub type PermissionRepository<'a> =
    persistence_core::Repository<'a, crate::entity::access_control::Permission>;
pub type RoleRepository<'a> = persistence_core::Repository<'a, crate::entity::Role>;
pub type UserRoleRepository<'a> = persistence_core::Repository<'a, crate::entity::access_control::UserRole>;
