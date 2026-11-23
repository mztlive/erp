//! 按集合别名包装 [`persistence_core::Repository`]。
//!
//! 领域特有方法是通用仓储上的扩展 trait。

pub type AccountCoreRepository<'a> = persistence_core::Repository<'a, crate::entity::AccountCore>;
pub type PersonQueryQualificationRepository<'a> =
    persistence_core::Repository<'a, crate::entity::person_directory::PersonQueryQualification>;
pub type AuditEventRepository<'a> =
    persistence_core::Repository<'a, crate::entity::access_control::AuditEvent>;
pub type RoleRepository<'a> = persistence_core::Repository<'a, crate::entity::Role>;

pub type PersonDataScopeRepository<'a> =
    persistence_core::Repository<'a, crate::entity::access_control::person_scope::PersonDataScope>;
