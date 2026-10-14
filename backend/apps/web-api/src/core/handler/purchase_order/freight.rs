//! 运费另计跟进的 HTTP 协议适配（§7.5.1）。

use application_core::AuditActor;
use axum::extract::{Path, State};
use axum::{Extension, Json};
use erp_processes::procure_to_pay::PurchaseOrderProcess;
use erp_procurement::dto::purchase_order::{WaivePurchaseFreightRequest, WaivePurchaseFreightResult};

use crate::app_state::AppState;
use crate::core::errors::Result;
use crate::core::response::ApiResponse;

#[permission_macros::permission(
    group = "采购单",
    group_desc = "采购单、采购提交与采购变更管理",
    desc = "标记供应商不收运费",
    resource = "purchase_order",
    action = "update"
)]
/// 标记采购单某商品的运费另计由供应商免收。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 采购单
/// * `req` - 销售行与原因
///
/// # 返回
/// 返回已标记的采购单与销售行。
pub async fn freight_waiver_create(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<WaivePurchaseFreightRequest>,
) -> Result<WaivePurchaseFreightResult> {
    let result =
        PurchaseOrderProcess::with_rbac(state.db(), state.rbac()).waive_freight(&id, req, &actor).await?;
    Ok(ApiResponse::ok_with_data(result))
}
