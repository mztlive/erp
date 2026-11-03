//! 采购付款安排的协议适配，不直接读写业务集合。
use application_core::AuditActor;
use axum::Extension;
use axum::extract::{Path, State};
use erp_procure_to_pay::procure_to_pay::{PurchaseOrderProcess, SchedulePaymentFollowUp};
use erp_read_models::purchase_center::payment_arrangement::PaymentArrangementView;

use super::purchase_reads;
use crate::http::errors::Result;
use crate::http::extractor::Json;
use crate::http::response::ApiResponse;
use crate::http::state::HttpState;
use crate::wiring::purchase_ports;

/// 查询当前采购的下一步付款安排。
/// # 参数
/// 应用状态、已认证账号及采购身份。
/// # 返回
/// 当前余额、关联申请与跟进日期。
/// # 错误
/// 无来源读取资格或数据读取失败时拒绝。
pub async fn payment_arrangement(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> Result<PaymentArrangementView> {
    Ok(ApiResponse::ok_with_data(purchase_reads(&state).payment_arrangement(&actor, &id).await?))
}
/// 保存采购付款跟进日期。
/// # 参数
/// 应用状态、账号、采购身份、任务版本与日期。
/// # 返回
/// 保存成功响应。
/// # 错误
/// 无付款申请权限、非任务负责人、版本冲突或日期非法时拒绝。
pub async fn schedule_payment_follow_up(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<SchedulePaymentFollowUp>,
) -> Result<()> {
    PurchaseOrderProcess::with_rbac(
        state.application().db(),
        state.application().rbac(),
        purchase_ports(state.application().db(), state.application().rbac()),
    )
    .schedule_payment_follow_up(&id, req, &actor)
    .await?;
    Ok(ApiResponse::ok())
}
