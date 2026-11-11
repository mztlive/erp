//! 当前客户查询凭证及对外稳定的范围元信息。
use std::sync::Arc;

use erp_core::common::time::Instant;
use erp_identity::CompiledAccess;

/// 当前读取阶段的授权凭证与展示元信息。
#[derive(Clone)]
pub struct CustomerResolvedScope {
    pub user_id: String,
    pub current: bool,
    pub scope_version: String,
    pub as_of: Instant,
    pub compiled: Arc<CompiledAccess>,
}
