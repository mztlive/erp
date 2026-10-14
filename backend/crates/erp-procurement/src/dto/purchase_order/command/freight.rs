//! 运费另计跟进命令合同。

use serde::{Deserialize, Serialize};
use validator::Validate;

/// 标记供应商不收运费的审计动作。
pub const WAIVE_FREIGHT_ACTION: &str = "purchase_order.waive_freight";

/// 标记某商品的运费另计由供应商免收。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Validate)]
pub struct WaivePurchaseFreightRequest {
    /// 运费另计的稳定销售行。
    #[validate(length(min = 1, max = 128, message = "请选择商品"))]
    pub sales_order_line_id: String,
    /// 原因，不超过 200 字。
    pub reason: String,
}

/// 标记结果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WaivePurchaseFreightResult {
    /// 采购单主键。
    pub purchase_order_id: String,
    /// 已标记的销售行。
    pub sales_order_line_id: String,
}
