//! 运费另计跟进的 HTTP 协议适配（§7.5.1）。

use application_core::AuditActor;
use axum::Extension;
use axum::extract::{Path, State};
use erp_procure_to_pay::procure_to_pay::PurchaseOrderProcess;
use erp_procurement::dto::purchase_order::{WaivePurchaseFreightRequest, WaivePurchaseFreightResult};

use crate::http::errors::Result;
use crate::http::extractor::Json;
use crate::http::response::ApiResponse;
use crate::http::state::HttpState;
use crate::wiring::purchase_ports;

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
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<WaivePurchaseFreightRequest>,
) -> Result<WaivePurchaseFreightResult> {
    let result = PurchaseOrderProcess::with_rbac(
        state.application().db(),
        state.application().rbac(),
        purchase_ports(state.application().db(), state.application().rbac()),
    )
    .waive_freight(&id, req, &actor)
    .await?;
    Ok(ApiResponse::ok_with_data(result))
}
