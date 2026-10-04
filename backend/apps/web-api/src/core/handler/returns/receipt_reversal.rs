//! 回款冲正 HTTP 协议适配。

use application_core::AuditActor;
use axum::extract::{Path, State};
use axum::{Extension, Json};
use erp_processes::reverse_flow::ReturnsProcess;
use erp_read_models::returns_center::ReturnsReadService;
use erp_read_models::returns_center::dto::ReceiptReversalView;
use erp_returns::dto::{
    CancelReceiptReversalApprovalRequest, CommitReceiptReversalRequest, CreateReceiptReversalRequest,
    PostReceiptReversalRequest, SubmitReceiptReversalRequest,
};
use erp_returns::service::ReturnsService;

use crate::app_state::AppState;
use crate::core::errors::Result;
use crate::core::response::ApiResponse;

#[permission_macros::permission(
    group = "退货退款",
    group_desc = "销售退货/拒收、采购退货与退款冲正管理（W05/W09/W11/W12）",
    desc = "查询回款冲正详情",
    resource = "receipt_reversal",
    action = "detail"
)]
/// 查询回款冲正详情。
///
/// # 参数
/// * `state` - 应用状态
/// * `id` - 冲正单 ID
///
/// # 返回
/// 返回冲正单视图。
///
/// # 错误
/// 业务处理或数据读取失败时返回对应错误。
pub async fn receipt_reversal_detail(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<ReceiptReversalView> {
    let view = ReturnsReadService::new(state.db()).receipt_reversal_detail(&id).await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "退货退款",
    group_desc = "销售退货/拒收、采购退货与退款冲正管理（W05/W09/W11/W12）",
    desc = "登记回款冲正草稿",
    resource = "receipt_reversal",
    action = "create"
)]
/// 登记回款冲正草稿（冲正单号唯一构成幂等去重；经办/复核分离）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `req` - 创建请求
///
/// # 返回
/// 返回新建冲正单视图。
///
/// # 错误
/// 业务处理或数据读取失败时返回对应错误。
pub async fn receipt_reversal_create(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<CreateReceiptReversalRequest>,
) -> Result<ReceiptReversalView> {
    let view = ReturnsProcess::new(state.db())
        .with_object_read(state.approval_object_read())
        .create_receipt_reversal(req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "退货退款",
    group_desc = "销售退货/拒收、采购退货与退款冲正管理（W05/W09/W11/W12）",
    desc = "一次创建并提交回款冲正审批",
    resource = "receipt_reversal",
    action = "submit"
)]
/// 按原回款一次创建回款冲正并启动审批。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已认证的当前操作人
/// * `req` - 业务命令与期望版本
///
/// # 返回
/// 返回处理后的单据视图。
///
/// # 错误
/// 业务处理或数据读取失败时返回对应错误。
pub async fn receipt_reversal_commit(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<CommitReceiptReversalRequest>,
) -> Result<ReceiptReversalView> {
    let view = ReturnsProcess::new(state.db())
        .with_object_read(state.approval_object_read())
        .commit_receipt_reversal(req, &actor)
        .await?;
    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "退货退款",
    group_desc = "销售退货/拒收、采购退货与退款冲正管理（W05/W09/W11/W12）",
    desc = "提交回款冲正审批",
    resource = "receipt_reversal",
    action = "submit"
)]
/// 提交回款冲正并启动统一审批。客户端不得选择定义或审批人。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 冲正单 ID
/// * `req` - 提交请求（版本与幂等键）
///
/// # 返回
/// 返回提交后的冲正单视图。
///
/// # 错误
/// 业务处理或数据读取失败时返回对应错误。
pub async fn receipt_reversal_submit(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<SubmitReceiptReversalRequest>,
) -> Result<ReceiptReversalView> {
    let view = ReturnsProcess::new(state.db())
        .with_object_read(state.approval_object_read())
        .submit_receipt_reversal(&id, req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "退货退款",
    group_desc = "销售退货/拒收、采购退货与退款冲正管理（W05/W09/W11/W12）",
    desc = "撤回回款冲正审批",
    resource = "receipt_reversal",
    action = "cancel_approval"
)]
/// 撤回尚未最终通过的回款冲正审批。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 冲正单 ID
/// * `req` - 撤回请求（原因必填）
///
/// # 返回
/// 返回撤回后的冲正单视图。
///
/// # 错误
/// 业务处理或数据读取失败时返回对应错误。
pub async fn receipt_reversal_cancel_approval(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<CancelReceiptReversalApprovalRequest>,
) -> Result<ReceiptReversalView> {
    let view = ReturnsProcess::new(state.db())
        .with_object_read(state.approval_object_read())
        .cancel_receipt_reversal_approval(&id, req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "退货退款",
    group_desc = "销售退货/拒收、采购退货与退款冲正管理（W05/W09/W11/W12）",
    desc = "回款冲正过账",
    resource = "receipt_reversal",
    action = "post"
)]
/// 客户端直接过账失败关闭。过账只允许作为审批最终通过动作。
///
/// # 参数
/// * `_state` - 应用状态
/// * `_actor` - 已通过鉴权的审计操作人
/// * `_id` - 冲正单 ID
/// * `_req` - 过账请求（客户端不得据此形成资金事实）
///
/// # 错误
/// 始终返回冲突，防止 HTTP 旁路过账。
///
/// # 返回
/// 无成功返回；始终拒绝客户端直接过账。
pub async fn receipt_reversal_post(
    State(_state): State<AppState>,
    Extension(_actor): Extension<AuditActor>,
    Path(_id): Path<String>,
    Json(_req): Json<PostReceiptReversalRequest>,
) -> Result<ReceiptReversalView> {
    match ReturnsService::reject_receipt_reversal_client_post() {
        Err(error) => Err(error.into()),
        Ok(never) => match never {},
    }
}
