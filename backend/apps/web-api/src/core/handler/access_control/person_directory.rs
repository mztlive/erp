//! 人员查询目录协议适配。类别由路由固定，不接受客户端指定资源。

use application_core::AuditActor;
use axum::Extension;
use axum::extract::{Query, State};
use erp_identity::service::person_directory::PersonDirectoryService;
use erp_identity::{
    PersonDirectoryCategory, PersonDirectoryPage, PersonDirectoryQuery, PersonDirectorySelectedQuery,
};

use crate::app_state::AppState;
use crate::core::errors::Result;
use crate::core::response::ApiResponse;

/// 查询销售人员目录。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已认证操作人
/// * `query` - 目录搜索、分页和组织筛选
///
/// # 返回
/// 返回销售人员页。
///
/// # 错误
/// 参数、权限或读取失败时返回统一错误。
///
/// # 关键业务约束
/// 不接收合同、客户或其他业务列表条件。
#[permission_macros::permission(
    group = "人员目录",
    group_desc = "独立人员查询与资格维护",
    desc = "查询销售人员",
    resource = "sales_person",
    action = "list"
)]
pub async fn list_salespeople(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Query(query): Query<PersonDirectoryQuery>,
) -> Result<PersonDirectoryPage> {
    list_people(state, actor, PersonDirectoryCategory::Sales, query).await
}

/// 回显已选销售人员。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已认证操作人
/// * `query` - 已选账号 ID
///
/// # 返回
/// 返回当前仍可读的销售人员。
///
/// # 错误
/// 参数、权限或读取失败时返回统一错误。
#[permission_macros::permission(
    group = "人员目录",
    group_desc = "独立人员查询与资格维护",
    desc = "回显已选销售人员",
    resource = "sales_person",
    action = "list"
)]
pub async fn list_selected_salespeople(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Query(query): Query<PersonDirectorySelectedQuery>,
) -> Result<PersonDirectoryPage> {
    selected_people(state, actor, PersonDirectoryCategory::Sales, query).await
}

/// 查询采购负责人目录。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已认证操作人
/// * `query` - 目录搜索、分页和组织筛选
///
/// # 返回
/// 返回采购负责人页。
///
/// # 错误
/// 参数、权限或读取失败时返回统一错误。
#[permission_macros::permission(
    group = "人员目录",
    group_desc = "独立人员查询与资格维护",
    desc = "查询采购负责人",
    resource = "procurement_person",
    action = "list"
)]
pub async fn list_procurement_people(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Query(query): Query<PersonDirectoryQuery>,
) -> Result<PersonDirectoryPage> {
    list_people(state, actor, PersonDirectoryCategory::Procurement, query).await
}

/// 回显已选采购负责人。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已认证操作人
/// * `query` - 已选账号 ID
///
/// # 返回
/// 返回当前仍可读的采购负责人。
///
/// # 错误
/// 参数、权限或读取失败时返回统一错误。
#[permission_macros::permission(
    group = "人员目录",
    group_desc = "独立人员查询与资格维护",
    desc = "回显已选采购负责人",
    resource = "procurement_person",
    action = "list"
)]
pub async fn list_selected_procurement_people(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Query(query): Query<PersonDirectorySelectedQuery>,
) -> Result<PersonDirectoryPage> {
    selected_people(state, actor, PersonDirectoryCategory::Procurement, query).await
}

/// 查询后台人员目录。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已认证操作人
/// * `query` - 目录搜索、分页和组织筛选
///
/// # 返回
/// 返回后台人员页。
///
/// # 错误
/// 参数、权限或读取失败时返回统一错误。
#[permission_macros::permission(
    group = "人员目录",
    group_desc = "独立人员查询与资格维护",
    desc = "查询后台人员",
    resource = "business_person",
    action = "list"
)]
pub async fn list_business_people(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Query(query): Query<PersonDirectoryQuery>,
) -> Result<PersonDirectoryPage> {
    list_people(state, actor, PersonDirectoryCategory::Business, query).await
}

/// 回显已选后台人员。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已认证操作人
/// * `query` - 已选账号 ID
///
/// # 返回
/// 返回当前仍可读的后台人员。
///
/// # 错误
/// 参数、权限或读取失败时返回统一错误。
#[permission_macros::permission(
    group = "人员目录",
    group_desc = "独立人员查询与资格维护",
    desc = "回显已选后台人员",
    resource = "business_person",
    action = "list"
)]
pub async fn list_selected_business_people(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Query(query): Query<PersonDirectorySelectedQuery>,
) -> Result<PersonDirectoryPage> {
    selected_people(state, actor, PersonDirectoryCategory::Business, query).await
}

async fn list_people(
    state: AppState,
    actor: AuditActor,
    category: PersonDirectoryCategory,
    query: PersonDirectoryQuery,
) -> Result<PersonDirectoryPage> {
    let page = service(&state).list(actor, category, query).await?;
    Ok(ApiResponse::ok_with_data(page))
}

async fn selected_people(
    state: AppState,
    actor: AuditActor,
    category: PersonDirectoryCategory,
    query: PersonDirectorySelectedQuery,
) -> Result<PersonDirectoryPage> {
    let page = service(&state).selected(actor, category, query.ids.as_slice().to_vec()).await?;
    Ok(ApiResponse::ok_with_data(page))
}

fn service(state: &AppState) -> PersonDirectoryService {
    PersonDirectoryService::new(state.db(), state.rbac())
}

/// 在目标账号管理范围内读取查询资格及乐观锁版本。
#[permission_macros::permission(
    group = "人员目录",
    group_desc = "独立人员查询与资格维护",
    desc = "读取人员查询资格",
    resource = "person_query_qualification",
    action = "manage"
)]
pub async fn get_qualification(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    axum::extract::Path((category, account_id)): axum::extract::Path<(PersonDirectoryCategory, String)>,
) -> Result<Option<erp_identity::entity::person_directory::PersonQueryQualification>> {
    let row = service(&state).qualification(actor, category, account_id, None).await?;
    Ok(ApiResponse::ok_with_data(row))
}

/// 显式授予、终止或恢复查询资格，不变更账号角色或命令权限。
#[permission_macros::permission(
    group = "人员目录",
    group_desc = "独立人员查询与资格维护",
    desc = "维护人员查询资格",
    resource = "person_query_qualification",
    action = "manage"
)]
pub async fn change_qualification(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    axum::extract::Path((category, account_id)): axum::extract::Path<(PersonDirectoryCategory, String)>,
    axum::Json(change): axum::Json<erp_identity::service::person_directory::management::QualificationChange>,
) -> Result<Option<erp_identity::entity::person_directory::PersonQueryQualification>> {
    let row = service(&state).qualification(actor, category, account_id, Some(change)).await?;
    Ok(ApiResponse::ok_with_data(row))
}
