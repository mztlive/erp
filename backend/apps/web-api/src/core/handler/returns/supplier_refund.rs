//! 供应商退款 HTTP 协议适配。

use application_core::AuditActor;
use axum::extract::{Path, State};
use axum::{Extension, Json};
use erp_processes::reverse_flow::ReturnsProcess;
use erp_read_models::returns_center::ReturnsReadService;
use erp_read_models::returns_center::dto::SupplierRefundView;
use erp_returns::dto::{
    CancelSupplierRefundApprovalRequest, CommitSupplierRefundRequest, CreateSupplierRefundRequest,
    PostSupplierRefundRequest, SubmitSupplierRefundRequest,
};
use erp_returns::service::ReturnsService;

use crate::app_state::AppState;
use crate::core::errors::Result;
use crate::core::response::ApiResponse;

#[permission_macros::permission(
    group = "退货退款",
    group_desc = "销售退货/拒收、采购退货与退款冲正管理（W05/W09/W11/W12）",
    desc = "查询供应商退款详情",
    resource = "supplier_refund",
    action = "detail"
)]
/// 查询供应商退款详情。
///
/// # 参数
/// * `state` - 应用状态
/// * `id` - 退款单 ID
///
/// # 返回
/// 返回退款单视图。
///
/// # 错误
/// 业务处理或数据读取失败时返回对应错误。
pub async fn supplier_refund_detail(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<SupplierRefundView> {
    let view = ReturnsReadService::new(state.db()).supplier_refund_detail(&id).await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "退货退款",
    group_desc = "销售退货/拒收、采购退货与退款冲正管理（W05/W09/W11/W12）",
    desc = "登记供应商退款草稿",
    resource = "supplier_refund",
    action = "create"
)]
/// 登记供应商退款草稿（退款单号唯一构成幂等去重；经办/复核分离）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `req` - 创建请求
///
/// # 返回
/// 返回新建退款单视图。
///
/// # 错误
/// 业务处理或数据读取失败时返回对应错误。
pub async fn supplier_refund_create(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<CreateSupplierRefundRequest>,
) -> Result<SupplierRefundView> {
    let view = ReturnsProcess::new(state.db())
        .with_object_read(state.approval_object_read())
        .create_supplier_refund(req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "退货退款",
    group_desc = "销售退货/拒收、采购退货与退款冲正管理（W05/W09/W11/W12）",
    desc = "一次创建并提交供应商退款审批",
    resource = "supplier_refund",
    action = "submit"
)]
/// 按原付款一次创建供应商退款并启动审批。
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
pub async fn supplier_refund_commit(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<CommitSupplierRefundRequest>,
) -> Result<SupplierRefundView> {
    let view = ReturnsProcess::new(state.db())
        .with_object_read(state.approval_object_read())
        .commit_supplier_refund(req, &actor)
        .await?;
    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "退货退款",
    group_desc = "销售退货/拒收、采购退货与退款冲正管理（W05/W09/W11/W12）",
    desc = "提交供应商退款审批",
    resource = "supplier_refund",
    action = "submit"
)]
/// 提交供应商退款并启动统一审批。客户端不得选择定义或审批人。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 退款单 ID
/// * `req` - 提交请求（版本与幂等键）
///
/// # 返回
/// 返回提交后的退款单视图。
///
/// # 错误
/// 业务处理或数据读取失败时返回对应错误。
pub async fn supplier_refund_submit(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<SubmitSupplierRefundRequest>,
) -> Result<SupplierRefundView> {
    let view = ReturnsProcess::new(state.db())
        .with_object_read(state.approval_object_read())
        .submit_supplier_refund(&id, req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "退货退款",
    group_desc = "销售退货/拒收、采购退货与退款冲正管理（W05/W09/W11/W12）",
    desc = "撤回供应商退款审批",
    resource = "supplier_refund",
    action = "cancel_approval"
)]
/// 撤回尚未最终通过的供应商退款审批。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 退款单 ID
/// * `req` - 撤回请求（原因必填）
///
/// # 返回
/// 返回撤回后的退款单视图。
///
/// # 错误
/// 业务处理或数据读取失败时返回对应错误。
pub async fn supplier_refund_cancel_approval(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<CancelSupplierRefundApprovalRequest>,
) -> Result<SupplierRefundView> {
    let view = ReturnsProcess::new(state.db())
        .with_object_read(state.approval_object_read())
        .cancel_supplier_refund_approval(&id, req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "退货退款",
    group_desc = "销售退货/拒收、采购退货与退款冲正管理（W05/W09/W11/W12）",
    desc = "供应商退款过账",
    resource = "supplier_refund",
    action = "post"
)]
/// 客户端直接过账失败关闭。过账只允许作为审批最终通过动作。
///
/// # 参数
/// * `_state` - 应用状态
/// * `_actor` - 已通过鉴权的审计操作人
/// * `_id` - 退款单 ID
/// * `_req` - 过账请求（客户端不得据此形成资金事实）
///
/// # 错误
/// 始终返回冲突，防止 HTTP 旁路过账。
///
/// # 返回
/// 无成功返回；始终拒绝客户端直接过账。
pub async fn supplier_refund_post(
    State(_state): State<AppState>,
    Extension(_actor): Extension<AuditActor>,
    Path(_id): Path<String>,
    Json(_req): Json<PostSupplierRefundRequest>,
) -> Result<SupplierRefundView> {
    match ReturnsService::reject_supplier_refund_client_post() {
        Err(error) => Err(error.into()),
        Ok(never) => match never {},
    }
}
