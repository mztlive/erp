//! 供应商供给 HTTP Handler。
//!
//! Handler 只做协议适配、成本字段授权与 Service 调用，不定义重复 DTO。

use axum::{
    extract::{Path, Query, State},
    Extension, Json,
};
use entities::Permission;
use services::{
    audit::AuditActor,
    supplier_offering::{
        CreateSupplierOfferingRequest, CreateSupplierOfferingResult, PageView, ReviseSupplierOfferingRequest,
        ReviseSupplierOfferingResult, SupplierOfferingListParams, SupplierOfferingService,
        SupplierOfferingView, UpdateSupplierOfferingAvailabilityRequest,
        UpdateSupplierOfferingAvailabilityResult,
    },
};

use crate::{
    app_state::AppState,
    core::{
        errors::{Error, Result},
        middleware::RbacSubject,
        response::ApiResponse,
    },
};

#[permission_macros::permission(
    group = "供应商供给",
    group_desc = "维护公司 SKU 的供应商供给、价格条款和可供状态",
    desc = "查询供应商供给列表",
    resource = "supplier_offering",
    action = "list"
)]
/// 查询供应商供给列表。
///
/// # 参数
/// * `state` - 应用状态
/// * `subject` - 当前授权主体
/// * `params` - 筛选与分页参数
///
/// # 返回
/// 返回供给分页列表；无成本权限时自动清除成本字段。
pub async fn list(
    State(state): State<AppState>,
    Extension(subject): Extension<RbacSubject>,
    Query(params): Query<SupplierOfferingListParams>,
) -> Result<PageView<SupplierOfferingView>> {
    let mut page = SupplierOfferingService::new(state.db()).list(&params).await?;
    if !can_view_costs(&state, &subject).await? {
        for item in &mut page.items {
            item.redact_costs();
        }
    }
    Ok(ApiResponse::ok_with_data(page))
}

#[permission_macros::permission(
    group = "供应商供给",
    group_desc = "维护公司 SKU 的供应商供给、价格条款和可供状态",
    desc = "新增供应商供给",
    resource = "supplier_offering",
    action = "create"
)]
/// 为公司 SKU 新增供应商供给。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 审计操作人
/// * `req` - 供给身份、条款和初始可供状态
///
/// # 返回
/// 返回新供给及首版事实主键。
pub async fn create(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<CreateSupplierOfferingRequest>,
) -> Result<CreateSupplierOfferingResult> {
    let result = SupplierOfferingService::new(state.db())
        .create(req, &actor)
        .await?;
    Ok(ApiResponse::ok_with_data(result))
}

#[permission_macros::permission(
    group = "供应商供给",
    group_desc = "维护公司 SKU 的供应商供给、价格条款和可供状态",
    desc = "保存供应商供给条款",
    resource = "supplier_offering",
    action = "update"
)]
/// 追加供给商业条款修订。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 审计操作人
/// * `id` - 供给主键
/// * `req` - 新条款与期望版本
///
/// # 返回
/// 返回新修订号与供给状态。
pub async fn revise(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<ReviseSupplierOfferingRequest>,
) -> Result<ReviseSupplierOfferingResult> {
    let result = SupplierOfferingService::new(state.db())
        .revise(&id, req, &actor)
        .await?;
    Ok(ApiResponse::ok_with_data(result))
}

#[permission_macros::permission(
    group = "供应商供给",
    group_desc = "维护公司 SKU 的供应商供给、价格条款和可供状态",
    desc = "更新供应商可供状态",
    resource = "supplier_offering_availability",
    action = "update"
)]
/// 更新供给的实时可供状态与数量。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 审计操作人
/// * `id` - 供给主键
/// * `req` - 新可供事实
///
/// # 返回
/// 返回更新后的可供状态与投影版本。
pub async fn update_availability(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<UpdateSupplierOfferingAvailabilityRequest>,
) -> Result<UpdateSupplierOfferingAvailabilityResult> {
    let result = SupplierOfferingService::new(state.db())
        .update_availability(&id, req, &actor)
        .await?;
    Ok(ApiResponse::ok_with_data(result))
}

async fn can_view_costs(state: &AppState, subject: &RbacSubject) -> std::result::Result<bool, Error> {
    let permission = Permission::parse("supplier_offering_cost:detail")?;
    state
        .rbac()
        .enforce(&subject.0, &permission)
        .await
        .map_err(Into::into)
}
