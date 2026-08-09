//! 域 D10 `catalog`：product_category、product_brand、unit_of_measure、sku_attribute、
//! sku_attribute_value、product_category_attribute、product(+_revision、_revision_media)、
//! sku(+_revision)、sku_revision_attribute_value、voucher_category_profile_revision
//! （页面：W14、W21、W22）。
//!
//! 实体层无跨域依赖：只引用 `entities::ids` 的 ID newtype 与 `common` 基元。
//! 字段字典与唯一约束见数据模型 §6.3；公共字段归属按 §4.3 判定：
//! - 稳定主表（product_category、product_brand、unit_of_measure、sku_attribute、
//!   sku_attribute_value、product、sku）组合 [`crate::common::StableBase`]；
//! - 不可变修订表（product_revision、sku_revision、voucher_category_profile_revision）
//!   用 [`crate::common::RevisionBase`]（revision_no），正式版本按 §4.4 内联结构化
//!   快照字段（商品名称、规格、单位等），P1 定义并校验、P3 填充；
//! - 关系行表（product_category_attribute、product_revision_media、
//!   sku_revision_attribute_value）只用 `BaseModel` 持久化元数据。
//!
//! 规格签名与身份排序位是跨行判定逻辑，封装在 [`specification`] 值对象。

pub mod product;
pub mod product_brand;
pub mod product_category;
pub mod product_category_attribute;
pub mod product_kind;
pub mod product_revision;
pub mod product_revision_media;
pub mod sku;
pub mod sku_attribute;
pub mod sku_attribute_value;
pub mod sku_revision;
pub mod sku_revision_attribute_value;
pub mod specification;
pub mod status;
pub mod unit_of_measure;
pub mod voucher_category_profile_revision;

pub use product::Product;
pub use product_brand::ProductBrand;
pub use product_category::ProductCategory;
pub use product_category_attribute::ProductCategoryAttribute;
pub use product_kind::ProductKind;
pub use product_revision::ProductRevision;
pub use product_revision_media::ProductRevisionMedia;
pub use sku::Sku;
pub use sku_attribute::SkuAttribute;
pub use sku_attribute_value::SkuAttributeValue;
pub use sku_revision::SkuRevision;
pub use sku_revision_attribute_value::SkuRevisionAttributeValue;
pub use specification::{SpecSignatureEntry, EMPTY_SPEC_SIGNATURE};
pub use status::{EnableStatus, ListingStatus, ProductListingStatus};
pub use unit_of_measure::UnitOfMeasure;
pub use voucher_category_profile_revision::VoucherCategoryProfileRevision;

// 域内 ID newtype 的统一出口（实体层无跨域依赖，只引用 entities::ids）。
pub use crate::ids::{
    ProductBrandId, ProductCategoryAttributeId, ProductCategoryId, ProductId, ProductRevisionId,
    ProductRevisionMediaId, SkuAttributeId, SkuAttributeValueId, SkuId, SkuRevisionAttributeValueId,
    SkuRevisionId, UnitOfMeasureId, VoucherCategoryProfileRevisionId,
};
