//! 采购文件下载与邮件发送协议入口。
use application_core::AuditActor;
use axum::extract::{Path, State};
use axum::{Extension, Json};
use erp_processes::purchase_correspondence::{
    self, PurchaseEmailHistory, PurchaseEmailResult, PurchasePaper,
};
use erp_procurement::entity::purchase_email::SendPurchaseEmail;

use crate::app_state::AppState;
use crate::core::errors::Result;
use crate::core::response::ApiResponse;

#[permission_macros::permission(
    group = "采购单",
    group_desc = "采购单、采购提交与采购变更管理",
    desc = "下载供应商采购单",
    resource = "purchase_order",
    action = "detail"
)]
/// 下载当前保存内容的独立采购单文件。
/// # 参数
/// `state` 为应用状态，`actor` 为认证人，`id` 为采购单。
/// # 返回
/// 独立 HTML 文件名及正文。
/// # 错误
/// 对象不可见或读取失败时拒绝。
pub async fn purchase_paper(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> Result<PurchasePaper> {
    Ok(ApiResponse::ok_with_data(
        purchase_correspondence::download(state.db(), state.rbac(), &id, &actor).await?,
    ))
}
#[permission_macros::permission(
    group = "采购单",
    group_desc = "采购单、采购提交与采购变更管理",
    desc = "发送采购单邮件给供应商",
    resource = "purchase_order",
    action = "send_email"
)]
/// 发送已生效采购单，所有正文由服务端生成。
/// # 参数
/// `state`、`actor`、`id` 为应用身份上下文，`req` 为发件确认。
/// # 返回
/// 返回服务商受理状态；不声明送达。
/// # 错误
/// 无权限、配置缺失、版本变化或请求非法时拒绝。
pub async fn send_purchase_email(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<SendPurchaseEmail>,
) -> Result<PurchaseEmailResult> {
    let config = state.config_snapshot();
    Ok(ApiResponse::ok_with_data(
        purchase_correspondence::send(
            state.db(),
            state.rbac(),
            config.purchase_mail.as_ref(),
            &id,
            req,
            &actor,
        )
        .await?,
    ))
}

#[permission_macros::permission(
    group = "采购单",
    group_desc = "采购单、采购提交与采购变更管理",
    desc = "查看采购邮件发送记录",
    resource = "purchase_order",
    action = "detail"
)]
/// 读取本采购单的发送记录。
/// # 参数
/// `state`、`actor` 和 `id` 为应用身份及采购单上下文。
/// # 返回
/// 最新 20 次发件记录。
/// # 错误
/// 对象不可见或读取失败时拒绝。
pub async fn purchase_email_history(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> Result<Vec<PurchaseEmailHistory>> {
    Ok(ApiResponse::ok_with_data(
        purchase_correspondence::history(state.db(), state.rbac(), &id, &actor).await?,
    ))
}
