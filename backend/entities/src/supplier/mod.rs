//! 域 D09 `supplier`：supplier_account、supplier_commercial_profile_revision、
//! supplier_capability(+_revision)、supplier_qualification(+_revision)、
//! supplier_qualification_capability、supplier_rating_revision（页面：W14）。
//!
//! 实体层无跨域依赖：只引用 `entities::ids` 的 ID newtype 与 `common` 基元。
//! 字段字典与唯一约束见数据模型 §6.2；公共字段归属按 §4.3 判定：
//! - `supplier_account` / `supplier_capability` / `supplier_qualification`
//!   是「稳定基础资料」→ 组合 [`crate::common::StableBase`]；
//! - `supplier_commercial_profile_revision` / `supplier_capability_revision` /
//!   `supplier_qualification_revision` / `supplier_rating_revision` 是不可变修订
//!   → 组合 [`crate::common::RevisionBase`]，快照字段按 §2.2 / §4.4 内联
//!   （付款条件、能力、资质证照、评分等，P3 填充）；
//! - `supplier_qualification_capability` 是资质 ↔ 能力的纯关联行（§6.2）；
//! - 跨聚合校验（资质失效不得用于新供给/采购单等）留给 P3，注释标注条目号。

pub mod supplier_account;
pub mod supplier_capability;
pub mod supplier_capability_revision;
pub mod supplier_commercial_profile_revision;
pub mod supplier_qualification;
pub mod supplier_qualification_capability;
pub mod supplier_qualification_revision;
pub mod supplier_rating_revision;

pub use crate::ids::{
    SupplierAccountId, SupplierCapabilityId, SupplierCapabilityRevisionId,
    SupplierCommercialProfileRevisionId, SupplierQualificationCapabilityId, SupplierQualificationId,
    SupplierQualificationRevisionId, SupplierRatingRevisionId,
};
pub use supplier_account::{
    SupplierAccount, SupplierAccountData, SupplierAccountStatus, SupplierAccountUpdate,
};
pub use supplier_capability::{
    CapabilityCode, CapabilityStatus, SupplierCapability, SupplierCapabilityData, SupplierCapabilityUpdate,
};
pub use supplier_capability_revision::{SupplierCapabilityRevision, SupplierCapabilityRevisionData};
pub use supplier_commercial_profile_revision::{
    InvoiceType, ReconciliationCycle, SettlementMode, SupplierCommercialProfileRevision,
    SupplierCommercialProfileRevisionData,
};
pub use supplier_qualification::{
    QualificationStatus, QualificationType, SupplierQualification, SupplierQualificationData,
    SupplierQualificationUpdate,
};
pub use supplier_qualification_capability::{
    SupplierQualificationCapability, SupplierQualificationCapabilityData,
};
pub use supplier_qualification_revision::{SupplierQualificationRevision, SupplierQualificationRevisionData};
pub use supplier_rating_revision::{SupplierRating, SupplierRatingRevision, SupplierRatingRevisionData};
