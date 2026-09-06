//! Owned catalog repositories composed from persistence-core.

mod product;
mod product_brand;
mod product_category;
mod product_category_attribute;
mod product_revision;
mod product_revision_media;
mod sku;
mod sku_attribute;
mod sku_attribute_value;
mod sku_revision;
mod unit_of_measure;
mod voucher_category_profile_revision;

pub use product::ProductRepository;
pub use product_brand::ProductBrandRepository;
pub use product_category::ProductCategoryRepository;
pub use product_category_attribute::ProductCategoryAttributeRepository;
pub use product_revision::ProductRevisionRepository;
pub use product_revision_media::ProductRevisionMediaRepository;
pub use sku::SkuRepository;
pub use sku_attribute::SkuAttributeRepository;
pub use sku_attribute_value::SkuAttributeValueRepository;
pub use sku_revision::SkuRevisionRepository;
pub use unit_of_measure::UnitOfMeasureRepository;
pub use voucher_category_profile_revision::VoucherCategoryProfileRevisionRepository;
