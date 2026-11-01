//! 财务登记消费的询价费用来源窄合同，不授予询价或供应商访问权。
use serde::{Deserialize, Serialize};

/// 财务已办理费用与询价分摊的稳定关系。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InquiryFeeReference {
    pub inquiry_id: String,
    pub feedback_round: u32,
    pub fee_id: String,
    pub allocation_id: String,
}
