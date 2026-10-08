//! [`persistence_core::Repository`] 的集合别名。
//!
//! 领域方法是通用仓储上的扩展 trait。

pub type CustomerAccountRepository<'a> =
    persistence_core::Repository<'a, crate::entity::customer::CustomerAccount>;
pub type CustomerAssignmentRepository<'a> =
    persistence_core::Repository<'a, crate::entity::customer::CustomerAssignment>;
pub type CustomerProfileCommandRepository<'a> =
    persistence_core::Repository<'a, crate::entity::customer::CustomerProfileCommand>;
