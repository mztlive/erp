//! 来源余额按稳定需求/地址/费用身份跨反馈轮次累计。
mod fees;
mod quantities;
use erp_core::money::{Amount, Quantity};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InquiryReservationState {
    Reserved,
    Handled,
    Released,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InquiryQuantitySelection {
    pub line_id: String,
    #[serde(default)]
    pub allocation_id: Option<String>,
    #[serde(default)]
    pub price_id: Option<String>,
    pub quantity: Quantity,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InquiryQuantityReservation {
    pub reservation_id: String,
    pub sales_order_id: String,
    pub requirement_version: u32,
    pub feedback_round: u32,
    pub selections: Vec<InquiryQuantitySelection>,
    pub state: InquiryReservationState,
}

/// 一笔费用对一张销售的明确金额归属；额度跨反馈轮次持续存在。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InquiryFeeAllocation {
    pub allocation_id: String,
    pub sales_order_id: String,
    pub feedback_round: u32,
    pub fee_id: String,
    pub amount: Amount,
}

/// 正式采购或财务登记所占用的费用额度。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InquiryFeeReservation {
    pub reservation_id: String,
    pub allocation_id: String,
    pub fee_id: String,
    pub sales_order_id: String,
    pub feedback_round: u32,
    pub amount: Amount,
    pub state: InquiryReservationState,
    pub source_document_id: String,
    pub source_line_id: String,
}
