//! 域 D24 `supplier_catalog` 的 HTTP handler。
//!
//! Handler 只做协议适配：`Validate`（DTO 内联）→ Service 调用 → `ApiResponse`，
//! 直接复用 `services::supplier_catalog` 的 DTO，禁止重复定义同构类型、禁止直连数据库。

use axum::{
    extract::{Path, Query, State},
    Extension, Json,
};
use services::{
    audit::AuditActor,
    supplier_catalog::{
        ApproveSupplierProductMappingRequest, ApproveSupplierProductMappingResult,
        CreateSupplierCatalogProductRequest, CreateSupplierCatalogProductResult,
        CreateSupplierProductMappingRequest, CreateSupplierProductMappingResult, PageView,
        ReviseSupplierCatalogProductRequest, ReviseSupplierCatalogProductResult,
        ReviseSupplierOfferingRequest, ReviseSupplierOfferingResult, SupplierCatalogIntakeBatchListParams,
        SupplierCatalogIntakeBatchView, SupplierCatalogProductDetailView, SupplierCatalogProductListParams,
        SupplierCatalogProductView, SupplierCatalogService, SupplierCatalogSkuListParams,
        SupplierCatalogSkuView, SupplierOfferingListParams, SupplierOfferingView,
        SupplierProductMappingListParams, SupplierProductMappingView,
    },
};

use crate::{
    app_state::AppState,
    core::{errors::Result, response::ApiResponse},
};

#[permission_macros::permission(
    group = "供应商商品库",
    group_desc = "供应商商品、SKU、映射、供给与入库批次管理",
    desc = "查询供应商 SPU 列表",
    resource = "supplier_catalog_product",
    action = "list"
)]
/// 查询供应商 SPU 列表。
///
/// # 参数
/// * `state` - 应用状态
/// * `query` - 分页与筛选参数（扁平传递）
///
/// # 返回
/// 返回契约形状的分页视图（`items`/`total`/`page`/`page_size`）。
pub async fn supplier_catalog_product_list(
    State(state): State<AppState>,
    Query(params): Query<SupplierCatalogProductListParams>,
) -> Result<PageView<SupplierCatalogProductView>> {
    let page = SupplierCatalogService::new(state.db())
        .product_list(&params)
        .await?;

    Ok(ApiResponse::ok_with_data(page))
}

#[permission_macros::permission(
    group = "供应商商品库",
    group_desc = "供应商商品、SKU、映射、供给与入库批次管理",
    desc = "查询供应商 SPU 详情",
    resource = "supplier_catalog_product",
    action = "detail"
)]
/// 查询供应商 SPU 详情（修订历史、媒体、SKU 与映射）。
///
/// # 参数
/// * `state` - 应用状态
/// * `id` - 供应商 SPU ID
///
/// # 返回
/// 返回详情视图。
pub async fn supplier_catalog_product_detail(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<SupplierCatalogProductDetailView> {
    let view = SupplierCatalogService::new(state.db())
        .product_detail(&id)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "供应商商品库",
    group_desc = "供应商商品、SKU、映射、供给与入库批次管理",
    desc = "创建供应商商品（Excel/API/手工入库）",
    resource = "supplier_catalog_product",
    action = "create"
)]
/// 创建供应商商品（§8.4 第 1 条：批次幂等入库）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `req` - 创建请求
///
/// # 返回
/// 返回创建（或重放）结果。
pub async fn supplier_catalog_product_create(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<CreateSupplierCatalogProductRequest>,
) -> Result<CreateSupplierCatalogProductResult> {
    let view = SupplierCatalogService::new(state.db())
        .create_product(req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "供应商商品库",
    group_desc = "供应商商品、SKU、映射、供给与入库批次管理",
    desc = "保存供应商商品来源修订",
    resource = "supplier_catalog_product",
    action = "update"
)]
/// 保存供应商商品来源修订（详情即编辑，只追加来源修订）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 供应商 SPU ID
/// * `req` - 保存请求（携带期望修订号）
///
/// # 返回
/// 返回新修订号。
pub async fn supplier_catalog_product_revise(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<ReviseSupplierCatalogProductRequest>,
) -> Result<ReviseSupplierCatalogProductResult> {
    let view = SupplierCatalogService::new(state.db())
        .revise_product(&id, req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "供应商商品库",
    group_desc = "供应商商品、SKU、映射、供给与入库批次管理",
    desc = "查询供应商 SKU 列表",
    resource = "supplier_catalog_sku",
    action = "list"
)]
/// 查询供应商 SKU 列表。
///
/// # 参数
/// * `state` - 应用状态
/// * `query` - 分页与筛选参数
///
/// # 返回
/// 返回契约形状的分页视图。
pub async fn supplier_catalog_sku_list(
    State(state): State<AppState>,
    Query(params): Query<SupplierCatalogSkuListParams>,
) -> Result<PageView<SupplierCatalogSkuView>> {
    let page = SupplierCatalogService::new(state.db()).sku_list(&params).await?;

    Ok(ApiResponse::ok_with_data(page))
}

#[permission_macros::permission(
    group = "供应商商品库",
    group_desc = "供应商商品、SKU、映射、供给与入库批次管理",
    desc = "查询供应商 SKU 映射列表",
    resource = "supplier_product_mapping",
    action = "list"
)]
/// 查询供应商 SKU → 公司 SKU 映射列表。
///
/// # 参数
/// * `state` - 应用状态
/// * `query` - 分页与筛选参数
///
/// # 返回
/// 返回契约形状的分页视图。
pub async fn supplier_product_mapping_list(
    State(state): State<AppState>,
    Query(params): Query<SupplierProductMappingListParams>,
) -> Result<PageView<SupplierProductMappingView>> {
    let page = SupplierCatalogService::new(state.db())
        .mapping_list(&params)
        .await?;

    Ok(ApiResponse::ok_with_data(page))
}

#[permission_macros::permission(
    group = "供应商商品库",
    group_desc = "供应商商品、SKU、映射、供给与入库批次管理",
    desc = "创建供应商 SKU 映射",
    resource = "supplier_product_mapping",
    action = "create"
)]
/// 创建供应商 SKU → 公司 SKU 映射（初始 `PENDING`）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `req` - 创建请求
///
/// # 返回
/// 返回映射结果。
pub async fn supplier_product_mapping_create(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<CreateSupplierProductMappingRequest>,
) -> Result<CreateSupplierProductMappingResult> {
    let view = SupplierCatalogService::new(state.db())
        .create_mapping(req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "供应商商品库",
    group_desc = "供应商商品、SKU、映射、供给与入库批次管理",
    desc = "确认映射并登记双价供给",
    resource = "supplier_product_mapping",
    action = "approve"
)]
/// 确认映射并登记双价供给（入池，映射 `Active` + 供给修订原子写入）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 映射 ID
/// * `req` - 确认请求
///
/// # 返回
/// 返回供给结果。
pub async fn supplier_product_mapping_approve(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<ApproveSupplierProductMappingRequest>,
) -> Result<ApproveSupplierProductMappingResult> {
    let view = SupplierCatalogService::new(state.db())
        .approve_mapping(&id, req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "供应商商品库",
    group_desc = "供应商商品、SKU、映射、供给与入库批次管理",
    desc = "查询供给列表",
    resource = "supplier_offering",
    action = "list"
)]
/// 查询供给列表。
///
/// # 参数
/// * `state` - 应用状态
/// * `query` - 分页与筛选参数
///
/// # 返回
/// 返回契约形状的分页视图。
pub async fn supplier_offering_list(
    State(state): State<AppState>,
    Query(params): Query<SupplierOfferingListParams>,
) -> Result<PageView<SupplierOfferingView>> {
    let page = SupplierCatalogService::new(state.db())
        .offering_list(&params)
        .await?;

    Ok(ApiResponse::ok_with_data(page))
}

#[permission_macros::permission(
    group = "供应商商品库",
    group_desc = "供应商商品、SKU、映射、供给与入库批次管理",
    desc = "保存供给修订",
    resource = "supplier_offering",
    action = "update"
)]
/// 保存供给修订（改价/暂停/停止，形成新的不可变供给修订）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 供给稳定身份 ID
/// * `req` - 修订请求
///
/// # 返回
/// 返回新修订号与状态。
pub async fn supplier_offering_revise(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<ReviseSupplierOfferingRequest>,
) -> Result<ReviseSupplierOfferingResult> {
    let view = SupplierCatalogService::new(state.db())
        .revise_offering(&id, req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "供应商商品库",
    group_desc = "供应商商品、SKU、映射、供给与入库批次管理",
    desc = "查询供应商商品入库批次",
    resource = "supplier_catalog_intake_batch",
    action = "list"
)]
/// 查询供应商商品入库批次列表。
///
/// # 参数
/// * `state` - 应用状态
/// * `query` - 分页与筛选参数
///
/// # 返回
/// 返回契约形状的分页视图。
pub async fn supplier_catalog_intake_batch_list(
    State(state): State<AppState>,
    Query(params): Query<SupplierCatalogIntakeBatchListParams>,
) -> Result<PageView<SupplierCatalogIntakeBatchView>> {
    let page = SupplierCatalogService::new(state.db())
        .intake_batch_list(&params)
        .await?;

    Ok(ApiResponse::ok_with_data(page))
}
