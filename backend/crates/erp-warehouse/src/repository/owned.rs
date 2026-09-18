//! Collection-scoped aliases for [`persistence_core::Repository`].
//!
//! Domain-specific methods are extension traits on the generic repository.

pub type WarehouseRepository<'a> = persistence_core::Repository<'a, crate::entity::warehouse::Warehouse>;
pub type WarehouseRevisionRepository<'a> =
    persistence_core::Repository<'a, crate::entity::warehouse::WarehouseRevision>;
pub type WarehouseSkuPolicyRepository<'a> =
    persistence_core::Repository<'a, crate::entity::warehouse::WarehouseSkuPolicy>;
