//! 页面授权的脱敏状态投影。
pub use erp_supply::ports::jd_vop::authorization::{
    JdVopAuthorizationRequest, JdVopAuthorizationStatusQuery,
};
use serde::Serialize;
/// 页面可缓存的脱敏状态；authorized只表达本地凭据可用，健康检查另行执行。
#[derive(Debug, Clone, Serialize)]
pub struct JdVopAuthorizationStatus {
    pub state: &'static str,
    pub version: Option<u64>,
    pub expires_at: Option<i64>,
    pub operation_id: Option<String>,
    pub operation_state: Option<&'static str>,
    pub can_authorize: bool,
    pub can_configure_tokens: bool,
    pub recovery_after: Option<i64>,
    pub message: &'static str,
}

impl JdVopAuthorizationStatus {
    pub(super) fn empty(state: &'static str, can_authorize: bool, message: &'static str) -> Self {
        Self {
            state,
            version: None,
            expires_at: None,
            operation_id: None,
            operation_state: None,
            can_authorize,
            can_configure_tokens: false,
            recovery_after: None,
            message,
        }
    }
}
