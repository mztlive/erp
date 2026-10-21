//! 提交结果未知时使用原命令查询安全结果，不生成新命令。
use application_core::AuditActor;
use axum::Extension;
use axum::extract::{Path, State};
use erp_processes::sales_inquiry::dto::InquiryMutationView;
use serde::Deserialize;

use crate::app_state::AppState;
use crate::core::errors::Result;
use crate::core::extractor::Query;
use crate::core::response::ApiResponse;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InquiryRecoveryParams {
    pub action: String,
}

#[permission_macros::permission(
    group = "销售询价",
    group_desc = "询价需求与采购反馈协作",
    desc = "核对销售询价命令结果",
    resource = "sales_inquiry",
    action = "recover"
)]
/// 使用原命令键查询已提交结果，当前销售范围仍须有效。
/// # 参数
/// `state` 为应用，`actor` 为当前账号，路径为询价及原命令键，查询含原动作。
/// # 返回
/// 返回原安全结果；未查到返回空值，不自动重放。
/// # 错误
/// 动作、回执、当前授权或对象范围不成立时拒绝。
pub async fn sales_recover(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path((id, key)): Path<(String, String)>,
    Query(params): Query<InquiryRecoveryParams>,
) -> Result<Option<InquiryMutationView>> {
    Ok(ApiResponse::ok_with_data(
        state.sales_inquiry_process().recover(&actor, &id, &params.action, &key).await?,
    ))
}

#[permission_macros::permission(
    group = "销售询价",
    group_desc = "询价需求与采购反馈协作",
    desc = "核对新建询价命令结果",
    resource = "sales_inquiry",
    action = "recover"
)]
/// 新建响应丢失后按原命令键取得由服务端登记的询价身份。
/// # 参数
/// `state`、`actor` 为当前应用及账号，`key` 为原创建命令键。
/// # 返回
/// 返回原创建结果；没有回执时返回空值。
/// # 错误
/// 回执或当前销售资格失效时拒绝。
pub async fn sales_create_recover(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(key): Path<String>,
) -> Result<Option<InquiryMutationView>> {
    Ok(ApiResponse::ok_with_data(state.sales_inquiry_process().recover(&actor, "", "create", &key).await?))
}

#[permission_macros::permission(
    group = "采购询价",
    group_desc = "询价需求与采购反馈协作",
    desc = "核对采购询价命令结果",
    resource = "procurement_inquiry",
    action = "recover"
)]
/// 查询本人采购询价命令的安全结果，不恢复历史任务操作权。
/// # 参数
/// `state`、`actor` 为当前应用及账号，路径限定询价和原命令，查询含动作。
/// # 返回
/// 仅返回原安全回执，不包含供应商或内部报价。
/// # 错误
/// 回执归属、当前账号或核对资格失效时拒绝。
pub async fn procurement_recover(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path((id, key)): Path<(String, String)>,
    Query(params): Query<InquiryRecoveryParams>,
) -> Result<Option<InquiryMutationView>> {
    Ok(ApiResponse::ok_with_data(
        state.sales_inquiry_process().recover(&actor, &id, &params.action, &key).await?,
    ))
}
