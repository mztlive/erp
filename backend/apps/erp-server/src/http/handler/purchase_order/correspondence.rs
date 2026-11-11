//! 采购文件下载与邮件发送协议入口。
use application_core::AuditActor;
use axum::Extension;
use axum::extract::{Path, State};
use erp_procure_to_pay::purchase_correspondence::{
    self, PurchaseEmailHistory, PurchaseEmailResult, PurchasePaper,
};
use erp_procurement::entity::purchase_email::SendPurchaseEmail;

use crate::http::errors::Result;
use crate::http::extractor::Json;
use crate::http::response::ApiResponse;
use crate::http::state::HttpState;
use crate::wiring::MongoPurchaseDataScope;

/// 下载当前保存内容的独立采购单文件。
/// # 参数
/// `state` 为应用状态，`actor` 为认证人，`id` 为采购单。
/// # 返回
/// 独立 HTML 文件名及正文。
/// # 错误
/// 对象不可见或读取失败时拒绝。
pub async fn purchase_paper(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> Result<PurchasePaper> {
    Ok(ApiResponse::ok_with_data(
        purchase_correspondence::download(
            state.application().db(),
            MongoPurchaseDataScope::shared(state.application().db(), state.application().rbac()),
            &id,
            &actor,
        )
        .await?,
    ))
}
/// 发送已生效采购单，所有正文由服务端生成。
/// # 参数
/// `state`、`actor`、`id` 为应用身份上下文，`req` 为发件确认。
/// # 返回
/// 返回服务商受理状态；不声明送达。
/// # 错误
/// 无权限、配置缺失、版本变化或请求非法时拒绝。
pub async fn send_purchase_email(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<SendPurchaseEmail>,
) -> Result<PurchaseEmailResult> {
    let config = state.application().config_snapshot();
    Ok(ApiResponse::ok_with_data(
        purchase_correspondence::send(
            state.application().db(),
            state.application().rbac(),
            MongoPurchaseDataScope::shared(state.application().db(), state.application().rbac()),
            config.purchase_mail.as_ref(),
            &id,
            req,
            &actor,
        )
        .await?,
    ))
}

/// 读取本采购单的发送记录。
/// # 参数
/// `state`、`actor` 和 `id` 为应用身份及采购单上下文。
/// # 返回
/// 最新 20 次发件记录。
/// # 错误
/// 对象不可见或读取失败时拒绝。
pub async fn purchase_email_history(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> Result<Vec<PurchaseEmailHistory>> {
    Ok(ApiResponse::ok_with_data(
        purchase_correspondence::history(
            state.application().db(),
            MongoPurchaseDataScope::shared(state.application().db(), state.application().rbac()),
            &id,
            &actor,
        )
        .await?,
    ))
}
