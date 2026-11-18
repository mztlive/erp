//! 原采购变更单完整编辑快照的协议入口。

use application_core::AuditActor;
use axum::extract::{Path, State};
use axum::{Extension, Json};
use erp_procure_to_pay::procure_to_pay::PurchaseOrderProcess;
use erp_procurement::dto::purchase_order::{PurchaseChangeDraftView, SavePurchaseChangeDraftRequest};

use crate::http::errors::Result;
use crate::http::response::ApiResponse;
use crate::http::state::HttpState;
use crate::wiring::purchase_ports;

/// 读取可原单修改重提的采购变更完整目标。
/// 返回当前版本、付款条件和全部冻结目标行及稳定行键。
/// 来源更新和提交资格不足、非草稿、基准变化或冻结内容缺失时拒绝。
pub async fn purchase_change_draft(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> Result<PurchaseChangeDraftView> {
    let view = PurchaseOrderProcess::new(
        state.application().db(),
        state.application().authorization(),
        purchase_ports(
            state.application().db(),
            state.application().rbac(),
            state.application().authorization(),
        ),
    )
    .change_draft(&id, &actor)
    .await?;
    Ok(ApiResponse::ok_with_data(view))
}

/// 保存采购变更的编辑内容，保留当前草稿或已驳回状态。
/// `req` 为当前版本、完整编辑目标和请求幂等键。
/// 返回已保存的采购变更草稿。
/// 权限、版本、编辑资格或输入校验失败时返回错误。
pub async fn purchase_change_save_draft(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<SavePurchaseChangeDraftRequest>,
) -> Result<PurchaseChangeDraftView> {
    let view = PurchaseOrderProcess::new(
        state.application().db(),
        state.application().authorization(),
        purchase_ports(
            state.application().db(),
            state.application().rbac(),
            state.application().authorization(),
        ),
    )
    .save_change_draft(&id, req, &actor)
    .await?;
    Ok(ApiResponse::ok_with_data(view))
}
