//! 供应商外部调用失败事实；重试与结果未知政策由 integration 拥有。
use serde::{Deserialize, Serialize};
/// API 与履约网关共用的稳定失败分类，不携带重试政策。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SupplierFailureClass {
    CapabilityGap,
    MappingError,
    BusinessRejected,
    TransientFailure,
    ResultUnknown,
    AuthSignature,
    RateLimited,
    OutOfOrder,
}
