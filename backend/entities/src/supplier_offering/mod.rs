//! 域 D24 `supplier_offering`：公司 SKU 的供应商供给关系、商业条款修订与实时可供投影。
//!
//! 公司 `Product`/`Sku` 是唯一商品主数据。本域不得复制供应商 SPU/SKU 商品主档，
//! 也不存在供应商商品到公司商品的映射。供应商侧订货身份直接属于供给稳定对象；
//! 价格、税率、起订量和有效期属于不可变修订；高频库存与可供状态属于独立投影。

mod availability;
mod command;
mod offering;
mod types;
pub mod write_data;

pub use availability::{SupplierOfferingAvailability, SupplierOfferingAvailabilityData};
pub use command::{SupplierOfferingCommand, SupplierOfferingCommandData};
pub use offering::{
    PrefillSourceRefs, SupplierOffering, SupplierOfferingData, SupplierOfferingRevision,
    SupplierOfferingRevisionData,
};
pub use types::{
    AvailabilityInterruptionReason, AvailabilityStatus, OfferingRevisionImpact, OfferingSourceType,
    OfferingStatus,
};
