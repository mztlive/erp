//! 域 D27 `projection`：sales_order_projection(+_revision、_delivery)（页面：W23）。
//!
//! 实体层无跨域依赖：只引用 `entities::ids` 的 ID newtype 与 `common` 基元。
//! 字段字典与约束见数据模型 §6.16；公共字段归属按 §4.3 判定：
//! - `sales_order_projection` 是执行投影稳定身份（字典无状态与审计字段）→
//!   只用 `BaseModel` 持久化元数据，`sales_order_id`/`target_mall_id`/
//!   `current_acked_revision_id` 按字典精确建模，不硬套 StableBase；
//! - `sales_order_projection_revision` 是不可变执行投影版本（§4.4 结构化快照）→
//!   组合 [`crate::common::revision::RevisionBase`]，来源、ERP 销售版本、
//!   商城客户/卡券类目标识、表头履约期限、唯一明细执行字段（面额/卡张数/
//!   卡形态）与生效时间按字典内联；
//! - `sales_order_projection_delivery` 是投影下发记录 → 按字典精确建模；投递
//!   状态是普通记录字段（§7.7），不另设状态机。
//!
//! 销售单服务切换不是数据实体，不建任何专用表（§7.8）；存量单的第一份执行
//! 投影版本即切换时点的 ERP 当前销售单版本，不产生新的销售单版本（§10 矩阵、
//! phase-2 §8.5.4）。
//!
//! # 跨聚合不变式（P3，§8 无对应条目，契约来源：§6.16、phase-2 §8.x）
//! - `(sales_order_id, target_mall_id)` 唯一稳定投影；`(projection_id,
//!   revision_no)` 唯一；`(sales_order_revision_id, target_mall_id)` 唯一；
//!   幂等键为「ERP 销售单号 + ERP 销售单版本 + 目标商城」（§6.16）；
//! - 投影只包含销售单号、版本、客户、卡券类目、履约期限、面额、数量、卡形态
//!   和生效时间；不含成交金额、配赠、税率、开票和应收（§6.16，本模块的字段集
//!   即白名单）；
//! - 商城接收失败不回退销售单生效、版本或应收；商城确认前新单不得开始受该版
//!   影响的玩法、制卡、绑定和激活；变更版只阻断受新版本影响的执行（§6.16、
//!   phase-2 §8.2/§8.3）；
//! - 对账差异只创建差异任务，不自动覆盖 ERP 或商城正式事实（§6.16）；
//! - 存量单切换时以当时 ERP 当前销售单版本作为第一份执行投影版本，不单独登记
//!   基线（phase-2 §8.5.4）。

pub mod content_identity;
pub mod delivery;
pub mod delivery_guard;
pub mod revision;
pub mod sales_order_projection;

pub use content_identity::{ProjectionContentFingerprint, ProjectionContentSnapshot};
pub use delivery::{
    ProjectionDeliveryStatus, SalesOrderProjectionDelivery, SalesOrderProjectionDeliveryData,
    SalesOrderProjectionDeliveryUpdate,
};
pub use revision::{
    CardForm, ProjectionSource, SalesOrderProjectionRevision, SalesOrderProjectionRevisionData,
};
pub use sales_order_projection::{
    SalesOrderProjection, SalesOrderProjectionData, SalesOrderProjectionUpdate,
};

// 域内 ID newtype 的统一出口（实体层无跨域依赖，只引用 entities::ids）。
pub use crate::ids::{
    SalesOrderProjectionDeliveryId, SalesOrderProjectionId, SalesOrderProjectionRevisionId,
};
