//! 选品册客户售价命令。

use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::entity::sales_selection::SelectionPriceChange;

/// 在已确认准备批次内改价；金额与百分比均使用十进制字符串。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct UpdateSelectionPricesRequest {
    /// 册版本，改价和发布竞争同一版本。
    pub expected_version: u64,
    /// 当前预览的准备批次。
    pub batch_id: String,
    /// 同一请求重试必须沿用原键。
    pub idempotency_key: String,
    /// 单商品定价或全批次百分比加价。
    pub change: SelectionPriceChange,
}
