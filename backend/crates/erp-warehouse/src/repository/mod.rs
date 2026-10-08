//! 仓库 MongoDB 仓储与访问器。

pub mod extensions;
pub mod owned;
pub mod prelude;
pub mod warehouse;

pub use extensions::WarehouseExt;
pub use owned::{WarehouseRepository, WarehouseRevisionRepository, WarehouseSkuPolicyRepository};
pub use warehouse::{
    WarehouseDomainRepository, WarehouseFilter, WarehouseRepositoryExt, WarehouseRevisionFilter,
    WarehouseRevisionRepositoryExt, WarehouseRevisionRow, WarehouseRow, WarehouseSkuPolicyFilter,
    WarehouseSkuPolicyRepositoryExt, WarehouseSkuPolicyRow,
};

pub(crate) mod directory;
