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
/// 分层检查报告。
/// 越权、参数错误或存储故障返回统一错误信封。
pub async fn inspect(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Json(request): Json<AccessInspectionRequest>,
) -> Result<AccessInspectionView> {
    let view =
        AccessInspectionReadService::new(state.application().db(), state.application().authorization())
            .inspect(actor, request)
            .await?;
    Ok(ApiResponse::ok_with_data(view))
}
