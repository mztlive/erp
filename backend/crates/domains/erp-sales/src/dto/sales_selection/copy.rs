//! 选品册复制请求；客户之外的业务配置禁止从客户端覆盖。

use serde::{Deserialize, Serialize};
use validator::Validate;

/// 复制选品册；内容只从服务端原册读取。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct CopySalesSelectionBookletRequest {
    /// 原册版本，防止复制与预览不一致的内容。
    pub expected_version: u64,
    /// 重新选择的客户。
    #[validate(length(min = 1, max = 64))]
    pub customer_id: String,
    /// 同一次复制重试时沿用的幂等键。
    pub idempotency_key: String,
}
