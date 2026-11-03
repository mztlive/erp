//! 作废采购改单草稿的协议入口。

use application_core::AuditActor;
use axum::Extension;
use axum::extract::{Path, State};
use erp_procure_to_pay::procure_to_pay::PurchaseOrderProcess;
use erp_procurement::dto::purchase_order::VoidPurchaseChangeRequest;
use erp_read_models::purchase_center::dto::PurchaseChangeOrderView;

use crate::http::errors::Result;
use crate::http::extractor::Json;
use crate::http::response::ApiResponse;
use crate::http::state::HttpState;
use crate::wiring::purchase_ports;

/// 作废采购改单草稿（仅草稿态，乐观锁）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 变更单 ID
/// * `req` - 作废请求（期望版本）
///
/// # 返回
/// 返回作废后的变更单视图。
///
/// # 错误
/// 不可写、版本冲突或非草稿时返回错误。
pub async fn purchase_change_void(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<VoidPurchaseChangeRequest>,
) -> Result<PurchaseChangeOrderView> {
    let view = PurchaseOrderProcess::with_rbac(
        state.application().db(),
        state.application().rbac(),
        purchase_ports(state.application().db(), state.application().rbac()),
    )
    .with_object_read(state.application().approval_object_read())
    .void_change(&id, req, &actor)
    .await?;
    Ok(ApiResponse::ok_with_data(view))
}
