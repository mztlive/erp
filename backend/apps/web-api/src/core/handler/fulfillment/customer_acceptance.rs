//! 客户验收 HTTP 协议适配。

use application_core::AuditActor;
use axum::extract::{Path, Query, State};
use axum::{Extension, Json};
use erp_fulfillment::dto::{
    CommitCustomerAcceptanceRequest, CreateCustomerAcceptanceRequest, CustomerAcceptanceDetailView,
    CustomerAcceptanceListParams, CustomerAcceptanceView, PageView, PostCustomerAcceptanceRequest,
    ReverseCustomerAcceptanceRequest,
};
use erp_processes::Error as ProcessError;
use erp_processes::fulfillment_execution::customer_acceptance::CustomerAcceptanceProcess;
use erp_read_models::fulfillment_center::access::AcceptanceReadService;
use erp_read_models::fulfillment_center::dto::{AcceptanceEligibilityView, CommitCustomerAcceptanceView};

use super::service;
use crate::app_state::AppState;
use crate::core::errors::Result;
use crate::core::response::ApiResponse;

/// 装配客户验收命令入口，沿用对象读取授权与 RBAC 服务。
fn acceptance_process(state: &AppState) -> CustomerAcceptanceProcess {
    CustomerAcceptanceProcess::new(state.db(), service(state), state.rbac(), state.approval_object_read())
}

#[permission_macros::permission(
    group = "履约",
    group_desc = "采购入库、发货、交付、服务与客户验收管理",
    desc = "查询客户验收单列表",
    resource = "customer_acceptance",
    action = "list"
)]
/// 查询客户验收单列表（W06 验收历史视图）。
///
/// # 参数
/// * `state` - 应用状态
/// * `query` - 分页与筛选参数（`sales_order_id`/`status` 扁平传递）
///
/// # 返回
/// 返回契约形状的分页视图。
///
/// # 错误
/// 业务处理或数据读取失败时返回对应错误。
pub async fn customer_acceptance_list(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Query(params): Query<CustomerAcceptanceListParams>,
) -> Result<PageView<CustomerAcceptanceView>> {
    let page = AcceptanceReadService::new(state.db(), state.rbac()).list(&params, &actor).await?;

    Ok(ApiResponse::ok_with_data(page))
}

#[permission_macros::permission(
    group = "履约",
    group_desc = "采购入库、发货、交付、服务与客户验收管理",
    desc = "查询客户验收单详情",
    resource = "customer_acceptance",
    action = "detail"
)]
/// 查询客户验收单详情（表头 + 行 + 分配）。
///
/// # 参数
/// * `state` - 应用状态
/// * `id` - 验收单主键
///
/// # 返回
/// 返回验收单详情视图。
///
/// # 错误
/// 业务处理或数据读取失败时返回对应错误。
pub async fn customer_acceptance_detail(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> Result<CustomerAcceptanceDetailView> {
    let view = AcceptanceReadService::new(state.db(), state.rbac()).detail(&id, &actor).await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "履约",
    group_desc = "采购入库、发货、交付、服务与客户验收管理",
    desc = "创建客户验收单",
    resource = "customer_acceptance",
    action = "create"
)]
/// 创建客户验收单（草稿；分配在过账时校验并写入）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `req` - 创建请求（表头 + 行）
///
/// # 返回
/// 返回新建验收单的响应视图。
///
/// # 错误
/// 业务处理或数据读取失败时返回对应错误。
pub async fn customer_acceptance_create(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<CreateCustomerAcceptanceRequest>,
) -> Result<CustomerAcceptanceView> {
    let view = acceptance_process(&state).create_customer_acceptance(req, &actor).await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "履约",
    group_desc = "采购入库、发货、交付、服务与客户验收管理",
    desc = "原子登记客户验收",
    resource = "customer_acceptance",
    action = "post"
)]
/// 原子创建或替换客户验收草稿并立即过账。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `req` - 最终表头、行、履约分配和乐观锁版本
///
/// # 返回
/// 返回已过账验收单和过账后的剩余可验收事实。
///
/// # 错误
/// 业务处理或数据读取失败时返回对应错误。
pub async fn customer_acceptance_commit(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<CommitCustomerAcceptanceRequest>,
) -> Result<CommitCustomerAcceptanceView> {
    let view = acceptance_process(&state).commit_customer_acceptance(req, &actor).await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "履约",
    group_desc = "采购入库、发货、交付、服务与客户验收管理",
    desc = "过账客户验收",
    resource = "customer_acceptance",
    action = "post"
)]
/// 过账客户验收（验收行锁定 + 履约分配守恒 + 净验收上限校验同事务，§8.2 第 5 条）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 验收单主键
/// * `req` - 过账请求（逐行分配）
///
/// # 返回
/// 返回过账后的验收单视图。
///
/// # 错误
/// 业务处理或数据读取失败时返回对应错误。
pub async fn customer_acceptance_post(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<PostCustomerAcceptanceRequest>,
) -> Result<CustomerAcceptanceView> {
    let view = acceptance_process(&state).post_customer_acceptance(&id, req, &actor).await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "履约",
    group_desc = "采购入库、发货、交付、服务与客户验收管理",
    desc = "冲正客户验收",
    resource = "customer_acceptance",
    action = "reverse"
)]
/// 冲正客户验收（误录时新增反向验收与反向分配，不覆盖原事实）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 待冲正验收单主键
/// * `req` - 冲正请求（期望版本 + 原因）
///
/// # 返回
/// 返回新建反向验收单的视图。
///
/// # 错误
/// 业务处理或数据读取失败时返回对应错误。
pub async fn customer_acceptance_reverse(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<ReverseCustomerAcceptanceRequest>,
) -> Result<CustomerAcceptanceView> {
    let view = acceptance_process(&state).reverse_customer_acceptance(&id, req, &actor).await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "履约",
    group_desc = "采购入库、发货、交付、服务与客户验收管理",
    desc = "查询客户验收工作台",
    resource = "customer_acceptance",
    action = "list"
)]
/// 查询客户验收工作台（W06：销售行 + 可验收事实 + 验收历史）。
///
/// # 参数
/// * `state` - 应用状态
/// * `query` - 销售单（`sales_order_id` 必填）
///
/// # 返回
/// 返回验收工作台视图。
///
/// # 错误
/// 业务处理或数据读取失败时返回对应错误。
pub async fn customer_acceptance_eligible(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Query(params): Query<CustomerAcceptanceListParams>,
) -> Result<AcceptanceEligibilityView> {
    let sales_order_id = params
        .sales_order_id
        .clone()
        .ok_or_else(|| ProcessError::ValidationError("sales_order_id 不能为空".to_string()))?;
    let view = AcceptanceReadService::new(state.db(), state.rbac())
        .eligibility(sales_order_id.as_ref(), &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}
