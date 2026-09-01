//! 域 D26 `publication`：product_publication(+_revision、_revision_media)、
//! product_publication_delivery（页面：W22）。
//!
//! 实体层无跨域依赖：只引用 `entities::ids` 的 ID newtype 与 `common` 基元。
//! 字段字典与约束见数据模型 §6.15；公共字段归属按 §4.3 判定：
//! - `product_publication` 是稳定发布主表（字典含发布状态与当前生效版本）→
//!   组合 [`crate::common::stable::StableBase`]，`sku_id`/`target_mall_id` 按字典建模；
//! - `product_publication_revision` 是不可变发布版本（§4.4 结构化快照）→
//!   组合 [`crate::common::revision::RevisionBase`]，展示名称/规格/销售说明、
//!   销售价/税率/单位、可销售区域、商品级能力与生效区间按字典内联；
//! - `product_publication_revision_media` 是发布版本的受控媒体行 → 精确建模；
//! - `product_publication_delivery` 是发布投递记录 → 按字典精确建模；投递状态
//!   是普通记录字段（§7.7：投递状态由 `integration_error_task.status` 表达人工
//!   处理，不另设消息投递状态机），不做 [`crate::common::state::DocumentState`]。
//!
//! 全部实体为正式对象，不设业务软删除（§4.5）。
//!
//! # 跨聚合不变式（P3，§8 无对应条目，契约来源：§6.15、phase-2 §7.x）
//! - `(sku_id, target_mall_id)` 唯一稳定发布；`(product_publication_id,
//!   revision_no)` 唯一；`(sku_id, revision_no, target_mall_id)` 对外幂等（§6.15）；
//! - `(product_publication_revision_id, media_role, sort_no)` 唯一；同一发布版本
//!   只能有一张主图；媒体安全状态与保留期只作治理记录，不阻断发布（§6.15）；
//! - 发布后媒体引用不可原位替换，变更图片必须形成新发布修订（§6.15）；
//! - 商城成功确认前不得把该版本标记为商城已生效（§6.15、phase-2 §7.5）；
//! - 供应商不可供或数据过期时形成暂停发布版本或明确暂停动作（§6.15）；
//! - 已支付订单永久引用下单时 `product_publication_revision_id`（§6.15）；
//! - 商品发布数据与商城生效版本的核对只生成差异任务（phase-2 §13.3）。

pub mod content_identity;
pub mod delivery;
pub mod product_publication;
pub mod revision;
pub mod safety_pause;

pub use content_identity::{PublicationContentFingerprint, PublicationContentSnapshot};
pub use delivery::{
    ProductPublicationDelivery, ProductPublicationDeliveryData, ProductPublicationDeliveryUpdate,
    PublicationDeliveryStatus,
};
pub use product_publication::{
    ProductPublication, ProductPublicationData, ProductPublicationStatus, ProductPublicationUpdate,
};
pub use revision::{
    MediaRole, ProductCapability, ProductPublicationRevision, ProductPublicationRevisionData,
    ProductPublicationRevisionMedia, ProductPublicationRevisionMediaData, SaleStatus,
};
pub use safety_pause::{
    SafetyPauseAffectedPublication, SafetyPauseBlocker, SafetyPauseBlockerCode, SafetyPauseCause,
    SafetyPauseFollowUp, SafetyPauseSourceObjectType, SafetyPauseWorkItemRef, SystemSafetyPauseOperation,
    SystemSafetyPauseOperationData,
};

// 域内 ID newtype 的统一出口（实体层无跨域依赖，只引用 entities::ids）。
pub use crate::ids::{
    ProductPublicationDeliveryId, ProductPublicationId, ProductPublicationRevisionId,
    ProductPublicationRevisionMediaId,
};
