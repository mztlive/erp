//! 域 D09 `supplier` 的 HTTP handler。
//!
//! Handler 只做协议适配：`Validate`（DTO 内联）→ Service 调用 → `ApiResponse`，
//! 直接复用 `services::supplier` 的 DTO，禁止重复定义同构类型、禁止直连数据库。

use axum::{
    extract::{Path, Query, State},
    Extension, Json,
};
use services::audit::AuditActor;
use services::supplier::{
    capability::SupplierCapabilityService, qualification::SupplierQualificationService,
    rating::SupplierRatingService, CommercialProfileListParams, CommercialProfileView,
    CreateCommercialProfileRequest, CreateSupplierCapabilityRequest, CreateSupplierQualificationRequest,
    CreateSupplierRatingRequest, CreateSupplierRequest, PageView, SupplierCapabilityListParams,
    SupplierCapabilityView, SupplierDetailView, SupplierListParams, SupplierQualificationListParams,
    SupplierQualificationView, SupplierRatingListParams, SupplierRatingView, SupplierService, SupplierView,
    UpdateSupplierCapabilityRequest, UpdateSupplierQualificationRequest, UpdateSupplierRequest,
};

use crate::{
    app_state::AppState,
    core::{errors::Result, response::ApiResponse},
};

#[permission_macros::permission(
    group = "供应商",
    group_desc = "供应商角色、商务结算版本、能力与资质管理",
    desc = "查询供应商列表",
    resource = "supplier",
    action = "list"
)]
/// 查询供应商列表。
///
/// # 参数
/// * `state` - 应用状态
/// * `query` - 分页与筛选参数（`keyword`/`party_id`/`status` 扁平传递）
///
/// # 返回
/// 返回契约形状的分页视图（`items`/`total`/`page`/`page_size`）。
pub async fn supplier_list(
    State(state): State<AppState>,
    Query(params): Query<SupplierListParams>,
) -> Result<PageView<SupplierView>> {
    let page = SupplierService::new(state.db()).supplier_list(&params).await?;
    Ok(ApiResponse::ok_with_data(page))
}

#[permission_macros::permission(
    group = "供应商",
    group_desc = "供应商角色、商务结算版本、能力与资质管理",
    desc = "创建供应商",
    resource = "supplier",
    action = "create"
)]
/// 创建供应商（同事务建立供应商角色 + 首个商务结算版本）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `req` - 创建请求
///
/// # 返回
/// 返回新建供应商角色的响应视图。
pub async fn supplier_create(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<CreateSupplierRequest>,
) -> Result<SupplierView> {
    let view = SupplierService::new(state.db())
        .create_supplier(req, &actor)
        .await?;
    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "供应商",
    group_desc = "供应商角色、商务结算版本、能力与资质管理",
    desc = "查询供应商详情",
    resource = "supplier",
    action = "detail"
)]
/// 查询供应商详情（供应商 + 当前商务结算版本 + 主体编号）。
///
/// # 参数
/// * `state` - 应用状态
/// * `id` - 供应商角色 ID
///
/// # 返回
/// 返回供应商详情视图。
pub async fn supplier_detail(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<SupplierDetailView> {
    let view = SupplierService::new(state.db()).supplier_detail(&id).await?;
    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "供应商",
    group_desc = "供应商角色、商务结算版本、能力与资质管理",
    desc = "更新供应商",
    resource = "supplier",
    action = "update"
)]
/// 更新供应商角色（乐观锁）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 供应商角色 ID
/// * `req` - 更新请求（含期望版本）
///
/// # 返回
/// 返回更新后供应商角色的响应视图。
pub async fn supplier_update(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<UpdateSupplierRequest>,
) -> Result<SupplierView> {
    let view = SupplierService::new(state.db())
        .update_supplier(&id, req, &actor)
        .await?;
    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "供应商",
    group_desc = "供应商角色、商务结算版本、能力与资质管理",
    desc = "删除供应商",
    resource = "supplier",
    action = "delete"
)]
/// 软删除供应商角色。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 供应商角色 ID
///
/// # 返回
/// 返回统一成功信封。
pub async fn supplier_delete(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> Result<()> {
    SupplierService::new(state.db())
        .delete_supplier(&id, &actor)
        .await?;
    Ok(ApiResponse::ok())
}

#[permission_macros::permission(
    group = "供应商",
    group_desc = "供应商角色、商务结算版本、能力与资质管理",
    desc = "查询商务结算版本列表",
    resource = "supplier_commercial_profile",
    action = "list"
)]
/// 查询商务结算版本列表（版本链历史）。
///
/// # 参数
/// * `state` - 应用状态
/// * `id` - 供应商角色 ID
/// * `query` - 分页与筛选参数
///
/// # 返回
/// 返回契约形状的分页视图。
pub async fn supplier_commercial_profile_list(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(params): Query<CommercialProfileListParams>,
) -> Result<PageView<CommercialProfileView>> {
    let page = SupplierService::new(state.db())
        .commercial_profile_list(&id, &params)
        .await?;
    Ok(ApiResponse::ok_with_data(page))
}

#[permission_macros::permission(
    group = "供应商",
    group_desc = "供应商角色、商务结算版本、能力与资质管理",
    desc = "追加商务结算版本",
    resource = "supplier_commercial_profile",
    action = "create"
)]
/// 追加商务结算版本（推进当前版本指针）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 供应商角色 ID
/// * `req` - 创建请求
///
/// # 返回
/// 返回新版本与更新后供应商视图对。
pub async fn supplier_commercial_profile_create(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<CreateCommercialProfileRequest>,
) -> Result<(CommercialProfileView, SupplierView)> {
    let views = SupplierService::new(state.db())
        .create_commercial_profile(&id, req, &actor)
        .await?;
    Ok(ApiResponse::ok_with_data(views))
}

#[permission_macros::permission(
    group = "供应商",
    group_desc = "供应商角色、商务结算版本、能力与资质管理",
    desc = "查询供应商能力列表",
    resource = "supplier_capability",
    action = "list"
)]
/// 查询供应商能力列表。
///
/// # 参数
/// * `state` - 应用状态
/// * `id` - 供应商角色 ID
/// * `query` - 分页与筛选参数
///
/// # 返回
/// 返回契约形状的分页视图。
pub async fn supplier_capability_list(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(params): Query<SupplierCapabilityListParams>,
) -> Result<PageView<SupplierCapabilityView>> {
    let page = SupplierCapabilityService::new(state.db())
        .supplier_capability_list(&id, &params)
        .await?;
    Ok(ApiResponse::ok_with_data(page))
}

#[permission_macros::permission(
    group = "供应商",
    group_desc = "供应商角色、商务结算版本、能力与资质管理",
    desc = "创建供应商能力",
    resource = "supplier_capability",
    action = "create"
)]
/// 创建供应商能力（同事务建立能力 + 首版能力修订）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 供应商角色 ID
/// * `req` - 创建请求
///
/// # 返回
/// 返回新建能力的响应视图。
pub async fn supplier_capability_create(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<CreateSupplierCapabilityRequest>,
) -> Result<SupplierCapabilityView> {
    let view = SupplierCapabilityService::new(state.db())
        .create_supplier_capability(&id, req, &actor)
        .await?;
    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "供应商",
    group_desc = "供应商角色、商务结算版本、能力与资质管理",
    desc = "更新供应商能力",
    resource = "supplier_capability",
    action = "update"
)]
/// 更新供应商能力（乐观锁 + 追加能力修订）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 能力 ID
/// * `req` - 更新请求（含期望版本）
///
/// # 返回
/// 返回更新后能力的响应视图。
pub async fn supplier_capability_update(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<UpdateSupplierCapabilityRequest>,
) -> Result<SupplierCapabilityView> {
    let view = SupplierCapabilityService::new(state.db())
        .update_supplier_capability(&id, req, &actor)
        .await?;
    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "供应商",
    group_desc = "供应商角色、商务结算版本、能力与资质管理",
    desc = "查询供应商资质列表",
    resource = "supplier_qualification",
    action = "list"
)]
/// 查询供应商资质列表。
///
/// # 参数
/// * `state` - 应用状态
/// * `id` - 供应商角色 ID
/// * `query` - 分页与筛选参数
///
/// # 返回
/// 返回契约形状的分页视图。
pub async fn supplier_qualification_list(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(params): Query<SupplierQualificationListParams>,
) -> Result<PageView<SupplierQualificationView>> {
    let page = SupplierQualificationService::new(state.db())
        .supplier_qualification_list(&id, &params)
        .await?;
    Ok(ApiResponse::ok_with_data(page))
}

#[permission_macros::permission(
    group = "供应商",
    group_desc = "供应商角色、商务结算版本、能力与资质管理",
    desc = "创建供应商资质",
    resource = "supplier_qualification",
    action = "create"
)]
/// 创建供应商资质（同事务建立资质 + 首版修订 + 适用能力关联）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 供应商角色 ID
/// * `req` - 创建请求
///
/// # 返回
/// 返回新建资质的响应视图。
pub async fn supplier_qualification_create(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<CreateSupplierQualificationRequest>,
) -> Result<SupplierQualificationView> {
    let view = SupplierQualificationService::new(state.db())
        .create_supplier_qualification(&id, req, &actor)
        .await?;
    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "供应商",
    group_desc = "供应商角色、商务结算版本、能力与资质管理",
    desc = "更新供应商资质",
    resource = "supplier_qualification",
    action = "update"
)]
/// 更新供应商资质（乐观锁 + 追加资质修订）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 资质 ID
/// * `req` - 更新请求（含期望版本）
///
/// # 返回
/// 返回更新后资质的响应视图。
pub async fn supplier_qualification_update(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<UpdateSupplierQualificationRequest>,
) -> Result<SupplierQualificationView> {
    let view = SupplierQualificationService::new(state.db())
        .update_supplier_qualification(&id, req, &actor)
        .await?;
    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "供应商",
    group_desc = "供应商角色、商务结算版本、能力与资质管理",
    desc = "查询供应商评估版本列表",
    resource = "supplier_rating",
    action = "list"
)]
/// 查询供应商评估版本列表。
///
/// # 参数
/// * `state` - 应用状态
/// * `id` - 供应商角色 ID
/// * `query` - 分页与筛选参数
///
/// # 返回
/// 返回契约形状的分页视图。
pub async fn supplier_rating_list(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(params): Query<SupplierRatingListParams>,
) -> Result<PageView<SupplierRatingView>> {
    let page = SupplierRatingService::new(state.db())
        .supplier_rating_list(&id, &params)
        .await?;
    Ok(ApiResponse::ok_with_data(page))
}

#[permission_macros::permission(
    group = "供应商",
    group_desc = "供应商角色、商务结算版本、能力与资质管理",
    desc = "创建供应商评估版本",
    resource = "supplier_rating",
    action = "create"
)]
/// 创建供应商评估版本（期初评分只在首次版本允许填写）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 供应商角色 ID
/// * `req` - 创建请求
///
/// # 返回
/// 返回新建评估版本的响应视图。
pub async fn supplier_rating_create(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<CreateSupplierRatingRequest>,
) -> Result<SupplierRatingView> {
    let view = SupplierRatingService::new(state.db())
        .create_supplier_rating(&id, req, &actor)
        .await?;
    Ok(ApiResponse::ok_with_data(view))
}
