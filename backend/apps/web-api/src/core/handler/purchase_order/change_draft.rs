//! 原采购变更单完整编辑快照的协议入口。

use application_core::AuditActor;
use axum::Extension;
use axum::extract::{Path, State};
use erp_processes::procure_to_pay::PurchaseOrderProcess;
use erp_procurement::dto::purchase_order::PurchaseChangeDraftView;

use crate::app_state::AppState;
use crate::core::errors::Result;
use crate::core::response::ApiResponse;

#[permission_macros::permission(
    group = "采购单",
    group_desc = "采购单、采购提交与采购变更管理",
    desc = "读取原采购变更草稿",
    resource = "purchase_change_order",
    action = "submit"
)]
/// 读取可原单修改重提的采购变更完整目标。
///
/// # 参数
/// * `state` - 应用状态。
/// * `actor` - 已认证操作人。
/// * `id` - 原采购变更单主键。
/// # 返回
/// 返回当前版本、付款条件和全部冻结目标行及稳定行键。
/// # 错误
/// 来源更新和提交资格不足、非草稿、基准变化或冻结内容缺失时拒绝。
pub async fn purchase_change_draft(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> Result<PurchaseChangeDraftView> {
    let view = PurchaseOrderProcess::with_rbac(state.db(), state.rbac()).change_draft(&id, &actor).await?;
    Ok(ApiResponse::ok_with_data(view))
}
