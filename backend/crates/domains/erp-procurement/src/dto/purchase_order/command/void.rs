use application_core::non_blank;
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::Result;
use crate::entity::purchase_order::payload_fingerprint;

/// 作废草稿命令的服务端固定审计动作（同时参与请求指纹）。
pub const VOID_ACTION: &str = "purchase_order.void";

/// 作废采购草稿请求。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct VoidPurchaseOrderRequest {
    /// 期望的采购单乐观锁版本。
    #[validate(range(min = 1, message = "单据或资料版本无效，请刷新当前页面后再试"))]
    pub expected_lock_version: u64,
    /// 作废原因。
    #[validate(custom(function = "non_blank", message = "作废原因不能为空"))]
    #[validate(length(max = 512, message = "作废原因过长"))]
    pub reason: String,
    /// 业务请求幂等键。
    #[validate(custom(function = "non_blank", message = "本次操作缺少提交标识，请刷新页面后重新操作"))]
    #[validate(length(max = 128, message = "本次操作标识无效，请刷新页面后重新操作"))]
    pub idempotency_key: String,
}

impl VoidPurchaseOrderRequest {
    /// 计算作废请求指纹（不含原始幂等键）。
    ///
    /// 作废原因按实际审计语义去除首尾空白，期望版本仍属于请求载荷；摘要形态与存量收据一致，修改会破坏幂等兼容。
    ///
    /// 返回不包含原始幂等键的稳定 SHA-256 指纹。指纹载荷无法序列化时返回内部错误。
    ///
    /// `purchase_order_id` 为当前路径采购单 ID。
    pub fn request_fingerprint(&self, purchase_order_id: &str) -> Result<String> {
        let payload = VoidDraftFingerprintPayload {
            expected_lock_version: self.expected_lock_version,
            reason: self.reason.trim(),
        };
        payload_fingerprint(VOID_ACTION, purchase_order_id, &payload).map_err(Into::into)
    }
}

/// 作废请求指纹载荷。
#[derive(Serialize)]
struct VoidDraftFingerprintPayload<'a> {
    /// 客户端期望乐观锁版本。
    expected_lock_version: u64,
    /// 去除首尾空白后的作废原因。
    reason: &'a str,
}

/// 作废采购草稿结果。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct VoidPurchaseOrderResult {
    /// 采购单主键。
    pub purchase_order_id: String,
    /// 作废后的稳定状态。
    pub status: String,
    /// 作废后的乐观锁版本。
    pub lock_version: u64,
    /// 是否命中已经完成的作废结果。
    pub replayed: bool,
    /// 业务引用。
    pub reference: String,
}
