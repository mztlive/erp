//! 供应商预存款HTTP边界；所有归属、审批及资金事务经命名Process。
pub(crate) mod resolution;
use application_core::AuditActor;
use axum::Extension;
use axum::extract::{Path, State};
use erp_finance::entity::supplier_deposit::{
    SupplierDepositAccount, SupplierDepositAuthorization, SupplierDepositBatch,
    SupplierDepositRechargeRequest, SupplierDepositReconciliation, SupplierDepositReservation,
    SupplierDepositUsage,
};
use erp_read_models::finance::supplier_deposits::{DepositAccountDetail, DepositAccountRow};
use erp_supplier_orders::supplier_deposits::{
    ApproveDepositRecharge, AuthorizeDepositExecution, BindDepositConnection, DepositAccountList,
    DepositAccountQuery, DepositExpectedVersion, OpenDepositAccount, PayDepositRecharge,
    ReceiveDepositRecharge, ReconcileDepositAccount, RecordDepositDebit, RequestDepositRecharge,
    ReserveDepositExecution, ReturnDepositUsage, SettleDepositUsage, SupplierDepositManagementProcess,
};

use crate::app_state::AppState;
use crate::core::errors::Result;
use crate::core::extractor::{Json, Query};
use crate::core::response::ApiResponse;
fn process(state: &AppState) -> SupplierDepositManagementProcess {
    SupplierDepositManagementProcess::new(
        state.db(),
        state.sensitive_data().as_ref().clone(),
        state.rbac(),
        state.supplier_payment_tasks(),
    )
}

/// 查询预存款账户。
/// # 参数
/// 当前认证财务人员、服务器资源身份和明确命令。
/// # 返回
/// 返回完成主体与组织范围核验后的业务结果。
/// # 错误
/// 越范围、状态或版本冲突、证据不完整及资金不守恒时拒绝。
pub async fn list(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Query(command): Query<DepositAccountQuery>,
) -> Result<DepositAccountList<DepositAccountRow>> {
    Ok(ApiResponse::ok_with_data(process(&state).list(command, &actor).await?))
}

/// 读取预存款账户明细。
/// # 参数
/// 当前认证财务人员、服务器资源身份和明确命令。
/// # 返回
/// 返回完成主体与组织范围核验后的业务结果。
/// # 错误
/// 越范围、状态或版本冲突、证据不完整及资金不守恒时拒绝。
pub async fn detail(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> Result<DepositAccountDetail> {
    Ok(ApiResponse::ok_with_data(process(&state).detail(&id, &actor).await?))
}

/// 开立供应商预存款账户。
/// # 参数
/// 当前认证财务人员、服务器资源身份和明确命令。
/// # 返回
/// 返回完成主体与组织范围核验后的业务结果。
/// # 错误
/// 越范围、状态或版本冲突、证据不完整及资金不守恒时拒绝。
pub async fn open(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Json(command): Json<OpenDepositAccount>,
) -> Result<SupplierDepositAccount> {
    Ok(ApiResponse::ok_with_data(process(&state).open(command, &actor).await?))
}

/// 申请供应商预存款充值。
/// # 参数
/// 当前认证财务人员、服务器资源身份和明确命令。
/// # 返回
/// 返回完成主体与组织范围核验后的业务结果。
/// # 错误
/// 越范围、状态或版本冲突、证据不完整及资金不守恒时拒绝。
pub async fn request_recharge(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(command): Json<RequestDepositRecharge>,
) -> Result<SupplierDepositRechargeRequest> {
    Ok(ApiResponse::ok_with_data(process(&state).request_recharge(&id, command, &actor).await?))
}

/// 批准供应商预存款充值。
/// # 参数
/// 当前认证财务人员、服务器资源身份和明确命令。
/// # 返回
/// 返回完成主体与组织范围核验后的业务结果。
/// # 错误
/// 越范围、状态或版本冲突、证据不完整及资金不守恒时拒绝。
pub async fn approve_recharge(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path((id, request)): Path<(String, String)>,
    Json(command): Json<ApproveDepositRecharge>,
) -> Result<SupplierDepositRechargeRequest> {
    Ok(ApiResponse::ok_with_data(process(&state).approve_recharge(&id, &request, command, &actor).await?))
}

/// 登记充值银行付款。
/// # 参数
/// 当前认证财务人员、服务器资源身份和明确命令。
/// # 返回
/// 返回完成主体与组织范围核验后的业务结果。
/// # 错误
/// 越范围、状态或版本冲突、证据不完整及资金不守恒时拒绝。
pub async fn pay_recharge(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path((id, request)): Path<(String, String)>,
    Json(command): Json<PayDepositRecharge>,
) -> Result<SupplierDepositRechargeRequest> {
    Ok(ApiResponse::ok_with_data(process(&state).pay_recharge(&id, &request, command, &actor).await?))
}

/// 核对供应商充值到账。
/// # 参数
/// 当前认证财务人员、服务器资源身份和明确命令。
/// # 返回
/// 返回完成主体与组织范围核验后的业务结果。
/// # 错误
/// 越范围、状态或版本冲突、证据不完整及资金不守恒时拒绝。
pub async fn receive_recharge(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(command): Json<ReceiveDepositRecharge>,
) -> Result<SupplierDepositBatch> {
    Ok(ApiResponse::ok_with_data(process(&state).receive_recharge(&id, command, &actor).await?))
}

/// 授权有限额自动余额执行。
/// # 参数
/// 当前认证财务人员、服务器资源身份和明确命令。
/// # 返回
/// 返回完成主体与组织范围核验后的业务结果。
/// # 错误
/// 越范围、状态或版本冲突、证据不完整及资金不守恒时拒绝。
pub async fn authorize(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(command): Json<AuthorizeDepositExecution>,
) -> Result<SupplierDepositAuthorization> {
    Ok(ApiResponse::ok_with_data(process(&state).authorize(&id, command, &actor).await?))
}

/// 撤销自动余额执行授权。
/// # 参数
/// 当前认证财务人员、服务器资源身份和明确命令。
/// # 返回
/// 返回完成主体与组织范围核验后的业务结果。
/// # 错误
/// 越范围、状态或版本冲突、证据不完整及资金不守恒时拒绝。
pub async fn revoke(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path((id, authorization)): Path<(String, String)>,
    Json(command): Json<DepositExpectedVersion>,
) -> Result<SupplierDepositAuthorization> {
    Ok(ApiResponse::ok_with_data(process(&state).revoke(&id, &authorization, command, &actor).await?))
}

/// 登记预存款资金对账。
/// # 参数
/// 当前认证财务人员、服务器资源身份和明确命令。
/// # 返回
/// 返回完成主体与组织范围核验后的业务结果。
/// # 错误
/// 越范围、状态或版本冲突、证据不完整及资金不守恒时拒绝。
pub async fn reconcile(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(command): Json<ReconcileDepositAccount>,
) -> Result<SupplierDepositReconciliation> {
    Ok(ApiResponse::ok_with_data(process(&state).reconcile(&id, command, &actor).await?))
}

/// 占用执行组预存款。
/// # 参数
/// 当前认证财务人员、服务器资源身份和明确命令。
/// # 返回
/// 返回完成主体与组织范围核验后的业务结果。
/// # 错误
/// 越范围、状态或版本冲突、证据不完整及资金不守恒时拒绝。
pub async fn reserve(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(command): Json<ReserveDepositExecution>,
) -> Result<SupplierDepositReservation> {
    Ok(ApiResponse::ok_with_data(process(&state).reserve(&id, command, &actor).await?))
}

/// 核实预存款实际扣款。
/// # 参数
/// 当前认证财务人员、服务器资源身份和明确命令。
/// # 返回
/// 返回完成主体与组织范围核验后的业务结果。
/// # 错误
/// 越范围、状态或版本冲突、证据不完整及资金不守恒时拒绝。
pub async fn record_debit(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(command): Json<RecordDepositDebit>,
) -> Result<SupplierDepositUsage> {
    Ok(ApiResponse::ok_with_data(process(&state).record_debit(&id, command, &actor).await?))
}

/// 核销已核实预存款使用。
/// # 参数
/// 当前认证财务人员、服务器资源身份和明确命令。
/// # 返回
/// 返回完成主体与组织范围核验后的业务结果。
/// # 错误
/// 越范围、状态或版本冲突、证据不完整及资金不守恒时拒绝。
pub async fn settle(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path((id, usage)): Path<(String, String)>,
    Json(command): Json<SettleDepositUsage>,
) -> Result<SupplierDepositUsage> {
    Ok(ApiResponse::ok_with_data(process(&state).settle_usage(&id, &usage, command, &actor).await?))
}

/// 核实供应商预存款返款。
/// # 参数
/// 当前认证财务人员、服务器资源身份和明确命令。
/// # 返回
/// 返回完成主体与组织范围核验后的业务结果。
/// # 错误
/// 越范围、状态或版本冲突、证据不完整及资金不守恒时拒绝。
pub async fn return_usage(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(command): Json<ReturnDepositUsage>,
) -> Result<SupplierDepositUsage> {
    Ok(ApiResponse::ok_with_data(process(&state).return_usage(&id, command, &actor).await?))
}

/// 绑定连接资金账号。
/// # 参数
/// 当前认证财务人员、服务器资源身份和明确命令。
/// # 返回
/// 返回完成主体与组织范围核验后的业务结果。
/// # 错误
/// 越范围、状态或版本冲突、证据不完整及资金不守恒时拒绝。
pub async fn bind(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(command): Json<BindDepositConnection>,
) -> Result<()> {
    Ok(ApiResponse::ok_with_data(process(&state).bind_connection(&id, command, &actor).await?))
}
