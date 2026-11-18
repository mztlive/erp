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

use crate::http::errors::Result;
use crate::http::extractor::{Json, Query};
use crate::http::response::ApiResponse;
use crate::http::state::HttpState;
fn process(state: &HttpState) -> SupplierDepositManagementProcess {
    SupplierDepositManagementProcess::new(
        state.application().db(),
        state.application().sensitive_data().as_ref().clone(),
        state.application().authorization(),
        state.application().supplier_payment_tasks(),
    )
}

/// 在预存款账户页按当前财务身份的主体和组织范围查询账户；流程校验筛选与访问资格，不能用客户端指定的账户归属绕过范围。
pub async fn list(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Query(command): Query<DepositAccountQuery>,
) -> Result<DepositAccountList<DepositAccountRow>> {
    Ok(ApiResponse::ok_with_data(process(&state).list(command, &actor).await?))
}

/// 打开预存款账户时按路径账户 ID 获取明细；当前身份缺少主体或组织范围、账户归属不成立或读取失败时拒绝。
pub async fn detail(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> Result<DepositAccountDetail> {
    Ok(ApiResponse::ok_with_data(process(&state).detail(&id, &actor).await?))
}

/// 建立供应商预存款记账关系时开立账户；由财务流程核验主体、组织和账户身份冲突，不通过开立命令确认到账。
pub async fn open(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Json(command): Json<OpenDepositAccount>,
) -> Result<SupplierDepositAccount> {
    Ok(ApiResponse::ok_with_data(process(&state).open(command, &actor).await?))
}

/// 需要补充供应商预存款时，为指定账户提交金额与操作身份，建立待批准的充值申请；范围或输入不符时拒绝。
pub async fn request_recharge(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(command): Json<RequestDepositRecharge>,
) -> Result<SupplierDepositRechargeRequest> {
    Ok(ApiResponse::ok_with_data(process(&state).request_recharge(&id, command, &actor).await?))
}

/// 审核已提出的充值申请时，按账户和申请身份提交审批；申请人不能自行批准，主体范围或原状态变化时拒绝。
pub async fn approve_recharge(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path((id, request)): Path<(String, String)>,
    Json(command): Json<ApproveDepositRecharge>,
) -> Result<SupplierDepositRechargeRequest> {
    Ok(ApiResponse::ok_with_data(process(&state).approve_recharge(&id, &request, command, &actor).await?))
}

/// 充值批准后登记银行付款事实；申请未批准、银行账户不属于供应商、回单无权读取或版本变化时拒绝。
pub async fn pay_recharge(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path((id, request)): Path<(String, String)>,
    Json(command): Json<PayDepositRecharge>,
) -> Result<SupplierDepositRechargeRequest> {
    Ok(ApiResponse::ok_with_data(process(&state).pay_recharge(&id, &request, command, &actor).await?))
}

/// 充值已付款后凭供应商到账证据核实金额并形成资金批次；未付款、归属错误、交易重复或证据不符时拒绝。
pub async fn receive_recharge(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(command): Json<ReceiveDepositRecharge>,
) -> Result<SupplierDepositBatch> {
    Ok(ApiResponse::ok_with_data(process(&state).receive_recharge(&id, command, &actor).await?))
}

/// 在允许按预算自动使用余额前，按账户为执行建立有限额授权；无范围、缺少依据或没有限额条款时拒绝。
pub async fn authorize(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(command): Json<AuthorizeDepositExecution>,
) -> Result<SupplierDepositAuthorization> {
    Ok(ApiResponse::ok_with_data(process(&state).authorize(&id, command, &actor).await?))
}

/// 停止使用某项自动余额授权时，按账户、授权身份和期望版本撤销；跨账户、越权或版本冲突时拒绝。
pub async fn revoke(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path((id, authorization)): Path<(String, String)>,
    Json(command): Json<DepositExpectedVersion>,
) -> Result<SupplierDepositAuthorization> {
    Ok(ApiResponse::ok_with_data(process(&state).revoke(&id, &authorization, command, &actor).await?))
}

/// 财务核对预存款余额时登记对账证据；越权、时间范围不完整、同操作号载荷冲突或证据缺失时拒绝。
pub async fn reconcile(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(command): Json<ReconcileDepositAccount>,
) -> Result<SupplierDepositReconciliation> {
    Ok(ApiResponse::ok_with_data(process(&state).reconcile(&id, command, &actor).await?))
}

/// 执行组动用预存款前先占用额度；采购、供应商连接、账户、预算或审批授权不符时拒绝。
pub async fn reserve(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(command): Json<ReserveDepositExecution>,
) -> Result<SupplierDepositReservation> {
    Ok(ApiResponse::ok_with_data(process(&state).reserve(&id, command, &actor).await?))
}

/// 拿到实际扣款证据后为预存款账户登记使用事实；跨账户、越权、证据缺少稳定交易身份或写入失败时拒绝。
pub async fn record_debit(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(command): Json<RecordDepositDebit>,
) -> Result<SupplierDepositUsage> {
    Ok(ApiResponse::ok_with_data(process(&state).record_debit(&id, command, &actor).await?))
}

/// 试图用已核实的预存款扣款核销应付时，验证授权和实际使用上限；应付变化或校验失败时拒绝核销并保留原扣款。
pub async fn settle(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path((id, usage)): Path<(String, String)>,
    Json(command): Json<SettleDepositUsage>,
) -> Result<SupplierDepositUsage> {
    Ok(ApiResponse::ok_with_data(process(&state).settle_usage(&id, &usage, command, &actor).await?))
}

/// 供应商实际返款后以返款证据核实余额变动；超额、证据不完整或越范围时拒绝，不把订单取消通知当成到账。
pub async fn return_usage(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(command): Json<ReturnDepositUsage>,
) -> Result<SupplierDepositUsage> {
    Ok(ApiResponse::ok_with_data(process(&state).return_usage(&id, command, &actor).await?))
}

/// 供应商连接需要使用预存款时绑定指定资金账户；不允许跨供应商、越权或覆盖已绑定的其他账号。
pub async fn bind(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(command): Json<BindDepositConnection>,
) -> Result<()> {
    Ok(ApiResponse::ok_with_data(process(&state).bind_connection(&id, command, &actor).await?))
}
