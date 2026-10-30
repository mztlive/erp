//! 当前指定出纳在原批准额度内明确执行预存款，不冒充银行付款。

use application_core::AuditActor;
use axum::Extension;
use axum::extract::{Path, State};
use erp_finance::dto::payable::payment_request::ExecutePurchasePaymentRequestDeposit;
use erp_finance::entity::supplier_deposit::SupplierDepositReservation;

use super::process;
use crate::app_state::AppState;
use crate::core::errors::Result;
use crate::core::extractor::Json;
use crate::core::response::ApiResponse;

#[permission_macros::permission(
    group = "采购付款申请",
    group_desc = "采购申请、财务审核和批准额度内付款",
    desc = "在批准采购付款申请内执行预存款",
    resource = "purchase_payment_request",
    action = "pay"
)]
/// 由当前付款任务出纳明确占用本次全部执行组资金。
/// # 参数
/// 原付款申请、当前指定任务及稳定原命令身份。
/// # 返回
/// 返回资金占用；不增加正式已付，不提前关闭付款任务。
/// # 错误
/// 任务失效、越批准来源、账户或余额不合格时拒绝。
pub async fn execute(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(command): Json<ExecutePurchasePaymentRequestDeposit>,
) -> Result<Vec<SupplierDepositReservation>> {
    Ok(ApiResponse::ok_with_data(
        process(&state).execute_deposit(&id, command, &actor, state.sensitive_data().as_ref()).await?,
    ))
}
