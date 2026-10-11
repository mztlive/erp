//! 采购履约付款摘要沿正式任务资格读取。
use application_core::AuditActor;
use axum::Extension;
use axum::extract::{Path, State};
use erp_processes::adapters::workflow::work_item_authorization::WorkItemAuthorizationAdapter;
use erp_read_models::finance::document_files::{BusinessFinanceFiles, FulfillmentPaymentView};

use crate::app_state::AppState;
use crate::core::errors::Result;
use crate::core::response::ApiResponse;

#[permission_macros::permission(
    group = "统一待办",
    group_desc = "待办队列与责任处理",
    desc = "查看采购履约付款进度",
    resource = "work_item",
    action = "detail"
)]
/// 读取当前履约来源的采购付款和申请进度。
/// # 参数
/// 当前账号及履约任务身份。
/// # 返回
/// 仅属于当前采购单的付款摘要。
/// # 错误
/// 任务无权限、来源不匹配或资格变化时拒绝。
pub async fn purchase_payment_summary(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> Result<FulfillmentPaymentView> {
    let auth = WorkItemAuthorizationAdapter::new(state.db(), state.rbac());
    let view =
        BusinessFinanceFiles::new(state.db(), state.rbac()).fulfillment_payment(&actor, &id, &auth).await?;
    Ok(ApiResponse::ok_with_data(view))
}
