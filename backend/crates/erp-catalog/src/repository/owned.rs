//! Collection-scoped aliases for [`persistence_core::Repository`].
//!
//! Domain-specific methods are extension traits on the generic repository.

pub type ProductRepository<'a> = persistence_core::Repository<'a, crate::entity::catalog::Product>;
pub type ProductBrandRepository<'a> = persistence_core::Repository<'a, crate::entity::catalog::ProductBrand>;
pub type ProductCategoryRepository<'a> =
    persistence_core::Repository<'a, crate::entity::catalog::ProductCategory>;
pub type ProductCategoryAttributeRepository<'a> =
    persistence_core::Repository<'a, crate::entity::catalog::ProductCategoryAttribute>;
pub type ProductRevisionRepository<'a> =
    persistence_core::Repository<'a, crate::entity::catalog::ProductRevision>;
pub type ProductRevisionMediaRepository<'a> =
    persistence_core::Repository<'a, crate::entity::catalog::ProductRevisionMedia>;
pub type SkuRepository<'a> = persistence_core::Repository<'a, crate::entity::catalog::Sku>;
pub type SkuAttributeRepository<'a> = persistence_core::Repository<'a, crate::entity::catalog::SkuAttribute>;
pub type SkuAttributeValueRepository<'a> =
    persistence_core::Repository<'a, crate::entity::catalog::SkuAttributeValue>;
pub type SkuRevisionRepository<'a> = persistence_core::Repository<'a, crate::entity::catalog::SkuRevision>;
pub type UnitOfMeasureRepository<'a> =
    persistence_core::Repository<'a, crate::entity::catalog::UnitOfMeasure>;
pub type VoucherCategoryProfileRevisionRepository<'a> =
    persistence_core::Repository<'a, crate::entity::catalog::VoucherCategoryProfileRevision>;
