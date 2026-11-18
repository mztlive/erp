//! 财务资料协议适配，授权和来源读取由读模型完成。
use application_core::AuditActor;
use axum::Extension;
use axum::extract::{Path, State};
use erp_read_models::finance::funds_scope::{AllocationSource, AllocationSourceKind};

use crate::http::errors::Result;
use crate::http::response::ApiResponse;
use crate::http::state::HttpState;
use crate::wiring::funds_access;

/// 按分配目标返回财务来源资料。
/// 经财务功能与来源授权验证的窄资料响应。
/// 无权限、目标不存在或版本错配时拒绝。
pub async fn detail(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path((kind, id)): Path<(AllocationSourceKind, String)>,
) -> Result<AllocationSource> {
    let data = funds_access(state.application().db(), state.application().authorization())
        .allocation_source(&actor, kind, &id)
        .await?;
    Ok(ApiResponse::ok_with_data(data))
}
