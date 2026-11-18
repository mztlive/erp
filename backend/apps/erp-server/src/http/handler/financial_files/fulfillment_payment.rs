//! 采购履约付款摘要沿正式任务资格读取。
use std::sync::Arc;

use application_core::AuditActor;
use axum::Extension;
use axum::extract::{Path, State};
use erp_read_models::finance::document_files::{BusinessFinanceFiles, FulfillmentPaymentView};

use crate::http::errors::Result;
use crate::http::response::ApiResponse;
use crate::http::state::HttpState;

/// 读取当前履约来源的采购付款和申请进度。
/// 仅属于当前采购单的付款摘要。
/// 任务无权限、来源不匹配或资格变化时拒绝。
pub async fn purchase_payment_summary(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> Result<FulfillmentPaymentView> {
    let view = BusinessFinanceFiles::new(state.application().db(), state.application().authorization())
        .fulfillment_payment(&actor, &id, Arc::new(state.application().work_item_authorization()))
        .await?;
    Ok(ApiResponse::ok_with_data(view))
}
