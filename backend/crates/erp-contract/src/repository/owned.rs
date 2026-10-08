//! [`persistence_core::Repository`] 的集合范围别名。
//!
//! 领域专用方法是泛型仓储上的扩展 trait。

pub type ContractRepository<'a> = persistence_core::Repository<'a, crate::entity::contract::Contract>;

pub type ContractRevisionRepository<'a> =
    persistence_core::Repository<'a, crate::entity::contract::ContractRevision>;
