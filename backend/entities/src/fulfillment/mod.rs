//! 域 D16 `fulfillment`：采购入库、发货、电子交付、服务履约、客户验收与验收分配
//! （页面：W06 客户验收、W09 收货与发货/交付与代发）。
//!
//! 实体层不访问数据库或外部 I/O；采购来源履约资格只接收 Service 已完整加载的
//! 采购状态、付款快照和关联 ID，在本域内执行确定性校验。
//! 字段字典与约束见数据模型 §6.7；公共字段归属按 §4.3 判定：
//! - `electronic_delivery` / `service_fulfillment` 的字典含 `occurred_at` 等正式
//!   事实字段 → 组合 `FactBase`；其余单据按 §6.7 字典精确建模（`posted_at`/
//!   `shipped_at`/`accepted_at` 等独立时间字段）；
//! - 全部实体 `#[serde(flatten)] BaseModel`；正式事实（已过账/已确认/已冲正及
//!   分配）不设业务软删除（§4.5.1），草稿可逻辑删除（§4.5.2）；
//! - 履约地址等敏感值按 §4.5.5/P1 §2.1 建模为加密值 + 带密钥 HMAC 查询指纹
//!   两个字段，自定义 `Debug` 不泄漏明文（实现见 [`fingerprint`]）；
//! - 状态机按 §7.5：单据 `DRAFT → POSTED → REVERSED`（交付为 `SHIPPED/SIGNED`、
//!   电子与服务为 `CONFIRMED`），`REVERSED` 为不可逆终态；
//! - `PurchaseReceipt` 签署为 `NO_APPROVAL`，不得新增审批绑定字段或审批状态机；
//! - `Delivery` 签署为 `NO_APPROVAL`，不得新增审批绑定字段或审批状态机。

pub mod acceptance_fulfillment_allocation;
pub mod customer_acceptance;
pub mod delivery;
pub mod electronic_delivery;
mod fingerprint;
pub mod purchase_receipt;
pub mod service_fulfillment;

pub use crate::ids::{
    AcceptanceFulfillmentAllocationId, CustomerAcceptanceId, CustomerAcceptanceLineId, DeliveryId,
    DeliveryLineId, ElectronicDeliveryId, PurchaseReceiptId, PurchaseReceiptLineId, ServiceFulfillmentId,
};
pub use acceptance_fulfillment_allocation::{
    AcceptanceFulfillmentAllocation, AcceptanceFulfillmentAllocationData, AllocationAction,
    FulfillmentFactType,
};
pub use customer_acceptance::{
    AcceptanceResult, CustomerAcceptance, CustomerAcceptanceData, CustomerAcceptanceLine,
    CustomerAcceptanceLineData, CustomerAcceptanceState, CustomerAcceptanceUpdate,
};
pub use delivery::{
    Delivery, DeliveryData, DeliveryLine, DeliveryLineData, DeliveryState, DeliveryType, DeliveryUpdate,
};
pub use electronic_delivery::{
    ElectronicDelivery, ElectronicDeliveryData, ElectronicDeliveryState, ElectronicDeliveryUpdate,
    FulfillmentResult,
};
pub use purchase_receipt::{
    PurchaseFulfillmentEligibility, PurchaseReceipt, PurchaseReceiptData, PurchaseReceiptLine,
    PurchaseReceiptLineData, PurchaseReceiptState, PurchaseReceiptUpdate, QualityResult,
};
pub use service_fulfillment::{
    ServiceFulfillment, ServiceFulfillmentData, ServiceFulfillmentState, ServiceFulfillmentUpdate,
};
