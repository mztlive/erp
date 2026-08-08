//! 域 D24 `supplier_catalog`：supplier_catalog_product(+_revision、_revision_media)、
//! supplier_catalog_sku(+_revision)、supplier_product_mapping、supplier_catalog_intake_batch
//! (+_item)、supplier_offering(+_revision)（页面：W21）。
//!
//! 字段字典与唯一约束见数据模型 §6.14；公共字段归属按 §4.3 判定：
//! - `supplier_catalog_product` / `supplier_catalog_sku` / `supplier_offering`
//!   是稳定身份（SPU/SKU/供给关系）→ 组合 [`crate::common::StableBase`]，
//!   内容全部放不可变修订；
//! - `supplier_catalog_product_revision` / `supplier_catalog_sku_revision` /
//!   `supplier_offering_revision` 是不可变修订 → 组合 [`crate::common::RevisionBase`]，
//!   并按 §4.4 内联结构化快照（名称、规格、单位、来源属性、白名单 HMAC 等）；
//! - `supplier_catalog_product_revision_media` / `supplier_product_mapping` /
//!   `supplier_catalog_intake_batch`(+_item) 按 §6.14 字典精确建模，只组合 `BaseModel`。
//!
//! 实体层无跨域依赖：只引用 `entities::ids` 的 ID newtype 与 `common` 基元；
//! 跨行/跨聚合校验（同修订媒体 `(media_usage, sort_order)` 唯一、同一供应商 SKU
//! 同时点单一映射、有效供给有效期不重叠等）留给 P3，条目见各文件注释。

mod command;
mod intake;
mod mapping;
mod offering;
mod product;
mod sku;
mod types;

pub use command::{SupplierCatalogCommand, SupplierCatalogCommandData};
pub use intake::{
    IntakeBatchStatus, IntakeItemClassification, IntakeItemResult, SupplierCatalogIntakeBatch,
    SupplierCatalogIntakeBatchData, SupplierCatalogIntakeItem, SupplierCatalogIntakeItemData,
};
pub use mapping::{MappingStatus, SupplierProductMapping, SupplierProductMappingData};
pub use offering::{
    OfferingStatus, PrefillSourceRefs, SupplierOffering, SupplierOfferingData, SupplierOfferingRevision,
    SupplierOfferingRevisionData,
};
pub use product::{
    ArchiveStatus, MediaUsage, SupplierCatalogProduct, SupplierCatalogProductData,
    SupplierCatalogProductRevision, SupplierCatalogProductRevisionData, SupplierCatalogProductRevisionMedia,
    SupplierCatalogProductRevisionMediaData,
};
pub use sku::{
    AvailabilityStatus, SupplierCatalogSku, SupplierCatalogSkuData, SupplierCatalogSkuRevision,
    SupplierCatalogSkuRevisionData,
};
pub use types::{CatalogItemStatus, CatalogSourceType, SourceAttribute};
