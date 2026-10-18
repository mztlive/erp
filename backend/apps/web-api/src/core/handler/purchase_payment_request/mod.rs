//! 采购付款申请 HTTP 边界；来源、金额、任务和事务由组合层执行。

pub mod candidate_list;
pub mod credit;
pub mod materials;
pub mod payments;

use application_core::{AuditActor, PageView};
use axum::extract::{Path, Query, State};
use axum::{Extension, Json};
use erp_finance::dto::payable::payment_request::{
    CancelPurchasePaymentRequest, CreatePaymentRequestBatch, PurchasePaymentRequestQuery,
    SavePurchasePaymentRequest, SubmitPurchasePaymentRequest,
};
use erp_finance::entity::payable::PurchasePaymentRequest;
use erp_processes::adapters::MongoPurchaseDataScope;
use erp_processes::finance_posting::payable::PurchasePaymentRequestProcess;
use erp_read_models::finance::payment_request::{
    PurchasePaymentRequestCandidate, PurchasePaymentRequestReadService, PurchasePaymentRequestView,
};

use crate::app_state::AppState;
use crate::core::errors::Result;
use crate::core::response::ApiResponse;

fn reads(state: &AppState) -> PurchasePaymentRequestReadService {
    PurchasePaymentRequestReadService::new(
        state.db(),
        state.rbac(),
        MongoPurchaseDataScope::shared(state.db(), state.rbac()),
    )
}

fn process(state: &AppState) -> PurchasePaymentRequestProcess {
    PurchasePaymentRequestProcess::new(state.db())
        .with_rbac(state.rbac())
        .with_object_read(state.approval_object_read())
}

#[permission_macros::permission(
    group = "采购付款申请",
    group_desc = "采购申请、财务审核和批准额度内付款",
    desc = "查询采购付款申请",
    resource = "purchase_payment_request",
    action = "list"
)]
/// 仅交付全部来源获准的整单申请列表。
/// # 参数
/// 当前账号与有界筛选条件。
/// # 返回
/// 当前授权范围内的分页申请。
/// # 错误
/// 动作、范围或分页非法时拒绝。
pub async fn list(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Query(query): Query<PurchasePaymentRequestQuery>,
) -> Result<PageView<PurchasePaymentRequestView>> {
    Ok(ApiResponse::ok_with_data(reads(&state).list(&actor, &query).await?))
}

#[permission_macros::permission(
    group = "采购付款申请",
    group_desc = "采购申请、财务审核和批准额度内付款",
    desc = "查询可申请采购来源",
    resource = "purchase_payment_request",
    action = "create"
)]
/// 查询具备申请资格、已生效且有可用余额的采购来源。
/// # 参数
/// 当前账号与可选供应商筛选。
/// # 返回
/// 逐采购金额、继承主体及脱敏账户候选。
/// # 错误
/// 来源资格或参数非法时拒绝。
pub async fn candidates(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Query(query): Query<PurchasePaymentRequestQuery>,
) -> Result<Vec<PurchasePaymentRequestCandidate>> {
    Ok(ApiResponse::ok_with_data(reads(&state).candidates(&actor, query.supplier_id.as_deref()).await?))
}

#[permission_macros::permission(
    group = "采购付款申请",
    group_desc = "采购申请、财务审核和批准额度内付款",
    desc = "读取采购付款申请",
    resource = "purchase_payment_request",
    action = "detail"
)]
/// 按全部采购来源或当前申请任务资格读取完整申请。
/// # 参数
/// 当前账号与申请身份。
/// # 返回
/// 提交快照和当前余额分别呈现的申请视图。
/// # 错误
/// 来源或任务不可见时拒绝。
pub async fn detail(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> Result<PurchasePaymentRequestView> {
    Ok(ApiResponse::ok_with_data(reads(&state).detail(&actor, &id).await?))
}

#[permission_macros::permission(
    group = "采购付款申请",
    group_desc = "采购申请、财务审核和批准额度内付款",
    desc = "创建采购付款申请",
    resource = "purchase_payment_request",
    action = "create"
)]
/// 创建不占用额度的采购付款申请草稿。
/// # 参数
/// 当前账号与逐采购金额表单。
/// # 返回
/// 新草稿身份和版本。
/// # 错误
/// 来源、金额或主体不合法时拒绝。
pub async fn create(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<SavePurchasePaymentRequest>,
) -> Result<PurchasePaymentRequest> {
    Ok(ApiResponse::ok_with_data(process(&state).create(req, &actor).await?))
}

#[permission_macros::permission(
    group = "采购付款申请",
    group_desc = "采购申请、财务审核和批准额度内付款",
    desc = "修改采购付款申请草稿",
    resource = "purchase_payment_request",
    action = "update"
)]
/// 修改本人尚未批准的申请草稿。
/// # 参数
/// 当前账号、申请身份与所见版本。
/// # 返回
/// 新版本申请。
/// # 错误
/// 非本人、非法状态或版本冲突时拒绝。
pub async fn update(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<SavePurchasePaymentRequest>,
) -> Result<PurchasePaymentRequest> {
    Ok(ApiResponse::ok_with_data(process(&state).update(&id, req, &actor).await?))
}

#[permission_macros::permission(
    group = "采购付款申请",
    group_desc = "采购申请、财务审核和批准额度内付款",
    desc = "提交采购付款申请审核",
    resource = "purchase_payment_request",
    action = "submit"
)]
/// 原子冻结本次来源、账户并占用逐采购额度。
/// # 参数
/// 当前账号、申请身份、版本及稳定命令身份。
/// # 返回
/// 已启动独立财务审批的申请。
/// # 错误
/// 额度不足、来源变化、无合格审批人或版本冲突时拒绝。
pub async fn submit(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<SubmitPurchasePaymentRequest>,
) -> Result<PurchasePaymentRequest> {
    Ok(ApiResponse::ok_with_data(process(&state).submit(&id, req, &actor).await?))
}

#[permission_macros::permission(
    group = "采购付款申请",
    group_desc = "采购申请、财务审核和批准额度内付款",
    desc = "撤回采购付款申请",
    resource = "purchase_payment_request",
    action = "withdraw"
)]
/// 撤回本人审批中申请，释放占用并终止开放审批任务。
/// # 参数
/// 当前账号、申请、版本、原因及稳定命令身份。
/// # 返回
/// 已撤回申请。
/// # 错误
/// 非本人、非法状态或版本冲突时拒绝。
pub async fn withdraw(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<CancelPurchasePaymentRequest>,
) -> Result<PurchasePaymentRequest> {
    Ok(ApiResponse::ok_with_data(process(&state).withdraw(&id, req, &actor).await?))
}

#[permission_macros::permission(
    group = "采购付款申请",
    group_desc = "采购申请、财务审核和批准额度内付款",
    desc = "关闭采购付款申请未付部分",
    resource = "purchase_payment_request",
    action = "close"
)]
/// 关闭批准未付金额，保留原批准及实际付款历史。
/// # 参数
/// 当前账号、申请、版本、原因及稳定命令身份。
/// # 返回
/// 已关闭申请和逐行关闭金额。
/// # 错误
/// 无合法处理资格、版本冲突或付款并发变化时拒绝。
pub async fn close(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<CancelPurchasePaymentRequest>,
) -> Result<PurchasePaymentRequest> {
    Ok(ApiResponse::ok_with_data(process(&state).close(&id, req, &actor).await?))
}

#[permission_macros::permission(
    group = "采购付款申请",
    group_desc = "采购申请、财务审核和批准额度内付款",
    desc = "批量创建采购付款申请",
    resource = "purchase_payment_request",
    action = "create"
)]
/// 原子创建并提交跨供应商的多份独立申请，同时复验逐来源提交权限。
/// # 参数
/// 当前账号、完整批次及稳定操作号。
/// # 返回
/// 按传入顺序返回全部审批中申请。
/// # 错误
/// 任一申请非法时整批失败，同操作号异载荷拒绝。
pub async fn create_batch(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<CreatePaymentRequestBatch>,
) -> Result<Vec<PurchasePaymentRequest>> {
    Ok(ApiResponse::ok_with_data(process(&state).create_batch(req, &actor).await?))
}
