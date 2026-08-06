//! 域 D08 `customer` 的 HTTP handler。
//!
//! Handler 只做协议适配：`Validate`（DTO 内联）→ Service 调用 → `ApiResponse`，
//! 直接复用 `services::customer` 的 DTO，禁止重复定义同构类型、禁止直连数据库。

use axum::{
    extract::{Path, Query, State},
    Extension, Json,
};
use services::audit::AuditActor;
use services::customer::{
    assignment::CustomerAssignmentService, CreateCustomerRequest, CustomerAssignmentListParams,
    CustomerAssignmentRequest, CustomerAssignmentView, CustomerDetailView, CustomerListParams,
    CustomerService, CustomerView, PageView, UpdateCustomerRequest,
};

use crate::{
    app_state::AppState,
    core::{errors::Result, response::ApiResponse},
};

#[permission_macros::permission(
    group = "客户",
    group_desc = "客户角色与归属管理",
    desc = "查询客户列表",
    resource = "customer",
    action = "list"
)]
/// 查询客户列表。
///
/// # 参数
/// * `state` - 应用状态
/// * `query` - 分页与筛选参数（`keyword`/`party_id`/`status` 扁平传递）
///
/// # 返回
/// 返回契约形状的分页视图（`items`/`total`/`page`/`page_size`）。
pub async fn customer_list(
    State(state): State<AppState>,
    Query(params): Query<CustomerListParams>,
) -> Result<PageView<CustomerView>> {
    let page = CustomerService::new(state.db()).customer_list(&params).await?;
    Ok(ApiResponse::ok_with_data(page))
}

#[permission_macros::permission(
    group = "客户",
    group_desc = "客户角色与归属管理",
    desc = "创建客户",
    resource = "customer",
    action = "create"
)]
/// 创建客户（同事务建立客户角色 + 首条 OWNER 归属）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `req` - 创建请求
///
/// # 返回
/// 返回新建客户角色的响应视图。
pub async fn customer_create(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<CreateCustomerRequest>,
) -> Result<CustomerView> {
    let view = CustomerService::new(state.db())
        .create_customer(req, &actor)
        .await?;
    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "客户",
    group_desc = "客户角色与归属管理",
    desc = "查询客户详情",
    resource = "customer",
    action = "detail"
)]
/// 查询客户详情（客户 + 主体身份 + 当前生效 OWNER）。
///
/// # 参数
/// * `state` - 应用状态
/// * `id` - 客户角色 ID
///
/// # 返回
/// 返回客户详情视图。
pub async fn customer_detail(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<CustomerDetailView> {
    let view = CustomerService::new(state.db()).customer_detail(&id).await?;
    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "客户",
    group_desc = "客户角色与归属管理",
    desc = "更新客户",
    resource = "customer",
    action = "update"
)]
/// 更新客户角色（乐观锁）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 客户角色 ID
/// * `req` - 更新请求（含期望版本）
///
/// # 返回
/// 返回更新后客户角色的响应视图。
pub async fn customer_update(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<UpdateCustomerRequest>,
) -> Result<CustomerView> {
    let view = CustomerService::new(state.db())
        .update_customer(&id, req, &actor)
        .await?;
    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "客户",
    group_desc = "客户角色与归属管理",
    desc = "删除客户",
    resource = "customer",
    action = "delete"
)]
/// 软删除客户角色。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 客户角色 ID
///
/// # 返回
/// 返回统一成功信封。
pub async fn customer_delete(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> Result<()> {
    CustomerService::new(state.db())
        .delete_customer(&id, &actor)
        .await?;
    Ok(ApiResponse::ok())
}

#[permission_macros::permission(
    group = "客户",
    group_desc = "客户角色与归属管理",
    desc = "查询客户归属列表",
    resource = "customer_assignment",
    action = "list"
)]
/// 查询客户归属列表。
///
/// # 参数
/// * `state` - 应用状态
/// * `id` - 客户角色 ID
/// * `query` - 分页与筛选参数
///
/// # 返回
/// 返回契约形状的分页视图。
pub async fn customer_assignment_list(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(params): Query<CustomerAssignmentListParams>,
) -> Result<PageView<CustomerAssignmentView>> {
    let page = CustomerAssignmentService::new(state.db())
        .customer_assignment_list(&id, &params)
        .await?;
    Ok(ApiResponse::ok_with_data(page))
}

#[permission_macros::permission(
    group = "客户",
    group_desc = "客户角色与归属管理",
    desc = "调整客户归属",
    resource = "customer_assignment",
    action = "create"
)]
/// 调整客户归属（Assign 建立新归属并结束重叠旧归属；End 提前结束有效期）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 客户角色 ID
/// * `req` - 归属变更请求
///
/// # 返回
/// 返回本次变更涉及的归属行。
pub async fn customer_assignment_apply(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<CustomerAssignmentRequest>,
) -> Result<Vec<CustomerAssignmentView>> {
    let views = CustomerAssignmentService::new(state.db())
        .apply_assignment(&id, req, &actor)
        .await?;
    Ok(ApiResponse::ok_with_data(views))
}
