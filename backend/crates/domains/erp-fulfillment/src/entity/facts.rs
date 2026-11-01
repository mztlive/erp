//! 履约消费的外域最小事实；仅组合适配器解释提供方实体并显式构造。

use erp_core::ids::{PurchaseOrderRevisionLineId, SalesOrderRevisionLineId};
use erp_core::money::{Amount, Quantity, Rate};

/// 当前采购单对履约资格有意义的状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PurchaseOrderStatusFact {
    /// 采购已生效。
    Effective,
    /// 采购已部分执行。
    PartiallyExecuted,
    /// 其余采购状态，不允许履约。
    Other,
}

/// 当前采购版本冻结的先款门槛。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrepaymentRequirementFact {
    /// 是否启用先款后货。
    pub prepay_gate: bool,
    /// 最低有效付款金额。
    pub prepay_minimum_amount: Option<Amount>,
    /// 最低有效付款比例。
    pub prepay_minimum_ratio: Option<Rate>,
}

/// 履约消费的当前采购版本行身份与数量。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PurchaseRevisionLineFact {
    /// 采购版本行身份。
    pub id: PurchaseOrderRevisionLineId,
    /// 当前版本数量；物流费用行保留 None。
    pub quantity: Option<Quantity>,
}

/// 采购到销售的分配关联事实。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PurchaseAllocationFact {
    /// 被分配采购版本行。
    pub purchase_order_revision_line_id: PurchaseOrderRevisionLineId,
    /// 目标销售版本行。
    pub sales_order_revision_line_id: SalesOrderRevisionLineId,
}

/// 由累计有效收货派生、交给采购提供方写回的进度。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReceiptFulfillmentProgress {
    /// 尚未满足总采购数量。
    Partial,
    /// 正总采购数量已全部满足。
    Completed,
}

/// 验收规则派生、交给销售提供方写回的履约进度。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AcceptanceFulfillmentProgress {
    /// 尚未形成净验收。
    NotStarted,
    /// 已形成部分净验收。
    PartiallyFulfilled,
    /// 所有行的应履约数量均已满足。
    Completed,
}

/// 现场证据敏感级别的消费事实。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvidenceSensitivity {
    /// 普通资料，不能作为现场敏感凭证。
    General,
    /// 敏感资料。
    Sensitive,
    /// 高敏感资料。
    HighlySensitive,
}

/// 现场证据保留策略的消费事实。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvidenceRetention {
    /// 长期保留。
    LongTerm,
    /// 非长期保留，不满足现场凭证要求。
    Other,
}

/// 自动仓发行使用的入库预占最小事实。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReceiptReservationLineFact {
    /// 库存预占身份。
    pub reservation_id: erp_core::ids::StockReservationId,
    /// 目标稳定销售行。
    pub sales_order_line_id: erp_core::ids::SalesOrderLineId,
    /// 本次预占数量。
    pub reserved_quantity: Quantity,
}

/// 验收资格规则消费的销售当前版本行身份。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcceptanceSalesLineFact {
    /// 当前销售版本行主键。
    pub id: String,
    /// 跨版本稳定销售行主键。
    pub sales_order_line_id: erp_core::ids::SalesOrderLineId,
}

/// 验收资格规则消费的销售商品行应履约数量。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcceptanceSalesQuantityFact {
    /// 对应当前销售公共版本行。
    pub revision_line_id: SalesOrderRevisionLineId,
    /// 应履约数量。
    pub quantity: Quantity,
}
