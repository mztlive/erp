//! 域 D10 `catalog` 的 HTTP handler（SPU/SKU 与卡券类目组）。
//!
//! Handler 只做协议适配：`Validate`（DTO 内联）→ Service 调用 → `ApiResponse`，
//! 直接复用 `services::catalog` 的 DTO，禁止重复定义同构类型、禁止直连数据库。
//! 商品字典接口见同目录 `mod.rs`。

use axum::{
    extract::{Path, Query, State},
    Extension, Json,
};
use services::audit::AuditActor;
use services::catalog::{
    CatalogService, CreateProductRequest, CreateVoucherCategoryRequest, PageView, ProductListParams,
    ProductRevisionListParams, ProductRevisionView, ProductView, SkuListParams, SkuRevisionListParams,
    SkuRevisionView, SkuView, UpdateProductRequest, VoucherCategoryProfileListParams,
    VoucherCategoryProfileView,
};

use crate::{
    app_state::AppState,
    core::{errors::Result, response::ApiResponse},
};

#[permission_macros::permission(
    group = "商品与仓库",
    group_desc = "公司商品池、商品、类目、供应商与仓库基础资料",
    desc = "查询商品列表",
    resource = "product",
    action = "list"
)]
/// 查询商品（SPU）列表。
///
/// # 参数
/// * `state` - 应用状态
/// * `query` - 分页与筛选参数（`product_no`/`product_kind`/`status` 扁平传递）
///
/// # 返回
/// 返回契约形状的分页视图（`items`/`total`/`page`/`page_size`）。
pub async fn product_list(
    State(state): State<AppState>,
    Query(params): Query<ProductListParams>,
) -> Result<PageView<ProductView>> {
    let page = CatalogService::new(state.db()).product_list(&params).await?;

    Ok(ApiResponse::ok_with_data(page))
}

#[permission_macros::permission(
    group = "商品与仓库",
    group_desc = "公司商品池、商品、类目、供应商与仓库基础资料",
    desc = "创建商品",
    resource = "product",
    action = "create"
)]
/// 创建商品（SPU + 首个商品修订 + 媒体 + 全部 SKU 行，跨集合事务）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `req` - 创建请求（商品身份 + 修订快照 + SKU 规格行）
///
/// # 返回
/// 返回新建商品的响应视图。
pub async fn product_create(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<CreateProductRequest>,
) -> Result<ProductView> {
    let view = CatalogService::new(state.db())
        .product_create(req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "商品与仓库",
    group_desc = "公司商品池、商品、类目、供应商与仓库基础资料",
    desc = "规格编辑商品",
    resource = "product",
    action = "update"
)]
/// 规格编辑商品（按规范化签名分类保留/新增/重新启用/移除，跨集合事务）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 商品 ID
/// * `req` - 规格编辑请求（含期望版本与修订后全部 SKU 行）
///
/// # 返回
/// 返回编辑后商品的响应视图。
pub async fn product_update(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<UpdateProductRequest>,
) -> Result<ProductView> {
    let view = CatalogService::new(state.db())
        .product_update(&id, req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "商品与仓库",
    group_desc = "公司商品池、商品、类目、供应商与仓库基础资料",
    desc = "查询商品修订列表",
    resource = "product_revision",
    action = "list"
)]
/// 查询商品修订列表。
///
/// # 参数
/// * `state` - 应用状态
/// * `query` - 分页与筛选参数（`product_id`/`status` 扁平传递）
///
/// # 返回
/// 返回契约形状的分页视图。
pub async fn product_revision_list(
    State(state): State<AppState>,
    Query(params): Query<ProductRevisionListParams>,
) -> Result<PageView<ProductRevisionView>> {
    let page = CatalogService::new(state.db())
        .product_revision_list(&params)
        .await?;

    Ok(ApiResponse::ok_with_data(page))
}

#[permission_macros::permission(
    group = "商品与仓库",
    group_desc = "公司商品池、商品、类目、供应商与仓库基础资料",
    desc = "查询SKU列表",
    resource = "sku",
    action = "list"
)]
/// 查询 SKU 列表。
///
/// # 参数
/// * `state` - 应用状态
/// * `query` - 分页与筛选参数（`sku_no`/`product_id`/`status` 扁平传递）
///
/// # 返回
/// 返回契约形状的分页视图。
pub async fn sku_list(
    State(state): State<AppState>,
    Query(params): Query<SkuListParams>,
) -> Result<PageView<SkuView>> {
    let page = CatalogService::new(state.db()).sku_list(&params).await?;

    Ok(ApiResponse::ok_with_data(page))
}

#[permission_macros::permission(
    group = "商品与仓库",
    group_desc = "公司商品池、商品、类目、供应商与仓库基础资料",
    desc = "查询SKU修订列表",
    resource = "sku_revision",
    action = "list"
)]
/// 查询 SKU 修订列表。
///
/// # 参数
/// * `state` - 应用状态
/// * `query` - 分页与筛选参数（`sku_id`/`name`/`barcode`/`status` 扁平传递）
///
/// # 返回
/// 返回契约形状的分页视图。
pub async fn sku_revision_list(
    State(state): State<AppState>,
    Query(params): Query<SkuRevisionListParams>,
) -> Result<PageView<SkuRevisionView>> {
    let page = CatalogService::new(state.db()).sku_revision_list(&params).await?;

    Ok(ApiResponse::ok_with_data(page))
}

#[permission_macros::permission(
    group = "商品与仓库",
    group_desc = "公司商品池、商品、类目、供应商与仓库基础资料",
    desc = "查询卡券类目列表",
    resource = "voucher_category_profile",
    action = "list"
)]
/// 查询卡券类目扩展修订列表。
///
/// # 参数
/// * `state` - 应用状态
/// * `query` - 分页与筛选参数（`sku_id`/`status` 扁平传递）
///
/// # 返回
/// 返回契约形状的分页视图。
pub async fn voucher_category_profile_list(
    State(state): State<AppState>,
    Query(params): Query<VoucherCategoryProfileListParams>,
) -> Result<PageView<VoucherCategoryProfileView>> {
    let page = CatalogService::new(state.db())
        .voucher_category_profile_list(&params)
        .await?;

    Ok(ApiResponse::ok_with_data(page))
}

#[permission_macros::permission(
    group = "商品与仓库",
    group_desc = "公司商品池、商品、类目、供应商与仓库基础资料",
    desc = "创建卡券类目",
    resource = "voucher_category_profile",
    action = "create"
)]
/// 原子创建卡券类目（商品 + 首个修订 + 唯一 SKU + [可选内联新建分类] + 卡券类目
/// 扩展修订，跨集合事务）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `req` - 创建请求（`voucher_no` 同时作为 `product_no`/`sku_no`；
///   `category_id` 与 `new_category` 二选一）
///
/// # 返回
/// 返回新建卡券类目扩展修订的响应视图。
pub async fn voucher_category_create(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<CreateVoucherCategoryRequest>,
) -> Result<VoucherCategoryProfileView> {
    let view = CatalogService::new(state.db())
        .voucher_category_create(req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}
