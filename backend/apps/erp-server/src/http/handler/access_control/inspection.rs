//! 权限检查协议入口，检查权及对象可见性由服务端重新证明。
use application_core::AuditActor;
use axum::Extension;
use axum::extract::State;
use erp_identity::dto::inspection::{AccessInspectionRequest, AccessInspectionView};
use erp_read_models::access_inspection::AccessInspectionReadService;

use crate::http::errors::Result;
use crate::http::extractor::Json;
use crate::http::response::ApiResponse;
use crate::http::state::HttpState;

/// 只读检查指定人员的业务访问条件。
/// # 参数
/// * `state` - 服务装配。
/// * `actor` - 已认证检查人。
/// * `request` - 检查目标，不接受客户端提供的授权事实。
/// # 返回
/// 分层检查报告。
/// # 错误
/// 越权、参数错误或存储故障返回统一错误信封。
pub async fn inspect(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Json(request): Json<AccessInspectionRequest>,
) -> Result<AccessInspectionView> {
    let view = AccessInspectionReadService::new(state.application().db(), state.application().rbac())
        .inspect(actor, request)
        .await?;
    Ok(ApiResponse::ok_with_data(view))
}
