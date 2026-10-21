//! 询价转单依据和费用归属；正式销售及采购继续使用原业务命令。
use application_core::AuditActor;
use axum::Extension;
use axum::extract::{Path, State};
use erp_processes::order_to_cash::SalesOrderCommandProcess;
use erp_processes::sales_inquiry::dto::InquiryMutationView;
use erp_processes::sales_inquiry::expense_read::{
    FinancialInquiryExpenseCandidates, InquiryExpenseCandidates, InquiryExpenseReadService,
};
use erp_processes::sales_inquiry::fee_allocation::{AllocateInquiryFeesCommand, InquiryFeeSalesView};
use erp_sales::dto::sales_order::inquiry_basis::InquirySalesBasis;
use serde::Deserialize;

use crate::app_state::AppState;
use crate::core::errors::Result;
use crate::core::extractor::{Json, Query};
use crate::core::response::ApiResponse;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InquirySalesBasisParams {
    pub round: u32,
}

#[permission_macros::permission(
    group = "成本费用",
    group_desc = "成本事实与费用登记",
    desc = "读取财务登记询价费用候选",
    resource = "cost_entry",
    action = "create"
)]
/// 在既有销售费用登记范围内读取 ERP 外费用来源。
/// # 参数
/// `state`、`actor` 为当前应用及财务账号，`id` 为生效来源销售。
/// # 返回
/// 返回费用、税率与可登记余额，不含采购收款或供应商资料。
/// # 错误
/// 财务登记权限、销售范围或费用来源版本失效时拒绝。
pub async fn financial_inquiry_candidates(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> Result<FinancialInquiryExpenseCandidates> {
    Ok(ApiResponse::ok_with_data(
        InquiryExpenseReadService::new(state.db(), state.rbac()).financial_candidates(&id, &actor).await?,
    ))
}

#[permission_macros::permission(
    group = "采购单",
    group_desc = "采购单、采购提交与采购变更管理",
    desc = "读取销售来源询价费用候选",
    resource = "purchase_order",
    action = "create"
)]
/// 在现有费用采购来源范围内读取明确分摊的询价费用。
/// # 参数
/// `state`、`actor` 为当前应用及采购，`id` 为生效来源销售单。
/// # 返回
/// 返回本销售的费用余额与必要付款依据，已完成任务不提供任何授权。
/// # 错误
/// 采购创建或费用转换资格、来源范围、版本失效时拒绝。
pub async fn expense_inquiry_candidates(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> Result<InquiryExpenseCandidates> {
    Ok(ApiResponse::ok_with_data(
        InquiryExpenseReadService::new(state.db(), state.rbac()).candidates(&id, &actor).await?,
    ))
}

#[permission_macros::permission(
    group = "销售询价",
    group_desc = "询价需求与采购反馈协作",
    desc = "读取询价转销售依据",
    resource = "sales_inquiry",
    action = "convert_sales"
)]
/// 取得当前已接受且仍有效的转销售依据，不自动使用供给价作为成交价。
/// # 参数
/// `state`、`actor` 为当前应用及账号，`id` 为询价，`round` 为已接受反馈轮次。
/// # 返回
/// 返回商品、稳定地址与来源可用数量，实际建单仍走销售创建命令。
/// # 错误
/// 来源状态、版本、数量、有效期或当前范围不满足时拒绝。
pub async fn sales_basis(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Query(params): Query<InquirySalesBasisParams>,
) -> Result<InquirySalesBasis> {
    Ok(ApiResponse::ok_with_data(
        SalesOrderCommandProcess::with_rbac(state.db(), state.rbac())
            .inquiry_sales_basis(&id, params.round, &actor)
            .await?,
    ))
}

#[permission_macros::permission(
    group = "销售询价",
    group_desc = "询价需求与采购反馈协作",
    desc = "查询询价费用归属销售",
    resource = "sales_inquiry",
    action = "detail"
)]
/// 读取本询价已有来源关联的销售摘要供费用归属。
/// # 参数
/// `state`、`actor` 为当前应用及账号，`id` 为询价。
/// # 返回
/// 返回最小销售身份和是否可归属费用，不授予普通销售详情访问权。
/// # 错误
/// 当前询价销售范围失效或读取失败时拒绝。
pub async fn fee_sales(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> Result<Vec<InquiryFeeSalesView>> {
    Ok(ApiResponse::ok_with_data(state.sales_inquiry_process().fee_sales(&id, &actor).await?))
}

#[permission_macros::permission(
    group = "销售询价",
    group_desc = "询价需求与采购反馈协作",
    desc = "分配询价费用归属",
    resource = "sales_inquiry",
    action = "convert_sales"
)]
/// 按金额明确费用在来源销售间的分配，跨反馈版本累计核算。
/// # 参数
/// `state`、`actor` 为当前应用及账号，`id` 为询价，`request` 为金额归属及原命令身份。
/// # 返回
/// 返回已提交的询价安全状态。
/// # 错误
/// 范围、接受版本、分配合计、来源销售或幂等载荷不满足时拒绝。
pub async fn allocate_fees(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(request): Json<AllocateInquiryFeesCommand>,
) -> Result<InquiryMutationView> {
    Ok(ApiResponse::ok_with_data(state.sales_inquiry_process().allocate_fees(&actor, &id, request).await?))
}
