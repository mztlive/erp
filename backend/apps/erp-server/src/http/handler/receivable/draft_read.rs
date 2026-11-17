//! 原回款编辑字段读取，沿用 submit 静态资格与完整资金来源授权。

use application_core::AuditActor;
use axum::Extension;
use axum::extract::{Path, State};
use erp_finance_posting::finance_posting::receivable::ReceivableProcess;
use erp_read_models::finance::dto::CustomerReceiptView;

use crate::http::errors::Result;
use crate::http::response::ApiResponse;
use crate::http::state::HttpState;
use crate::wiring::receivable_posting_ports;

/// 读取原登记人的完整回款，保留真实状态供未知提交确认。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 当前已认证原登记人
/// * `id` - 原回款主键
///
/// # 返回
/// 返回未裁剪的实际字段、版本、审批与拟核销分配；只有 Draft 可编辑。
///
/// # 错误
/// 当前身份、submit 权限、完整来源或原登记人校验不通过时拒绝。
pub async fn customer_receipt_draft(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> Result<CustomerReceiptView> {
    let view = ReceivableProcess::new(
        state.application().db(),
        receivable_posting_ports(state.application().db(), state.application().authorization()),
    )
    .customer_receipt_draft(&id, &actor)
    .await?;
    Ok(ApiResponse::ok_with_data(view))
}
