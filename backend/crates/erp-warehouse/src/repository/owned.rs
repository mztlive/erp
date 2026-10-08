//! [`persistence_core::Repository`] 的集合别名。
//!
//! 领域方法是泛型仓储上的扩展 trait。

pub type WarehouseRepository<'a> = persistence_core::Repository<'a, crate::entity::warehouse::Warehouse>;
pub type WarehouseRevisionRepository<'a> =
    persistence_core::Repository<'a, crate::entity::warehouse::WarehouseRevision>;
pub type WarehouseSkuPolicyRepository<'a> =
    persistence_core::Repository<'a, crate::entity::warehouse::WarehouseSkuPolicy>;
