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
    ProductListingView, ProductRevisionListParams, ProductRevisionView, ProductView, SellableSkuListParams,
    SellableSkuView, SkuListParams, SkuRevisionListParams, SkuRevisionView, SkuView,
    UpdateProductListingRequest, UpdateProductRequest, UpdateSkuListingRequest, UpdateVoucherCategoryRequest,
    VoucherCategoryProfileListParams, VoucherCategoryProfileView,
};

use crate::{
    app_state::AppState,
    core::{errors::Result, response::ApiResponse},
};

#[permission_macros::permission(
    group = "商品与仓库",
    group_desc = "公司商品池、商品、类目、供应商与仓库基础资料",
    desc = "查询公司商品池",
    resource = "sku",
    action = "list"
)]
/// 查询符合销售资格的公司 SKU 只读投影。
///
/// # 参数
/// * `state` - 应用状态
/// * `params` - 搜索、商品类型、资格业务日期与分页参数
///
/// # 返回
/// 返回公司商品池分页视图，不包含采购成本与供应商身份。
pub async fn sellable_sku_list(
    State(state): State<AppState>,
    Query(params): Query<SellableSkuListParams>,
) -> Result<PageView<SellableSkuView>> {
    let view = CatalogService::new(state.db()).sellable_sku_list(&params).await?;
    Ok(ApiResponse::ok_with_data(view))
}

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
    desc = "整组上下架商品SKU",
    resource = "product",
    action = "update"
)]
/// 一次切换商品下全部当前启用 SKU 的上架状态。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 商品稳定 ID
/// * `req` - 全部 SKU 的目标上架状态
///
/// # 返回
/// 返回 SPU 从当前启用 SKU 继承得到的上架状态与数量。
pub async fn product_listing_update(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<UpdateProductListingRequest>,
) -> Result<ProductListingView> {
    let view = CatalogService::new(state.db())
        .product_listing_update(&id, req, &actor)
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
/// * `query` - 分页与筛选参数（`sku_no`/`product_id`/`status`/`listing_status` 扁平传递）
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
    desc = "上下架单个SKU",
    resource = "product",
    action = "update"
)]
/// 切换单个 SKU 的上架状态。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - SKU 稳定 ID
/// * `req` - 期望版本与目标上架状态
///
/// # 返回
/// 返回更新后的 SKU 视图。
pub async fn sku_listing_update(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<UpdateSkuListingRequest>,
) -> Result<SkuView> {
    let view = CatalogService::new(state.db())
        .sku_listing_update(&id, req, &actor)
        .await?;
    Ok(ApiResponse::ok_with_data(view))
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

#[permission_macros::permission(
    group = "商品与仓库",
    group_desc = "公司商品池、商品、类目、供应商与仓库基础资料",
    desc = "更新卡券类目",
    resource = "voucher_category_profile",
    action = "update"
)]
/// 更新卡券类目名称与描述（按 SKU 稳定身份定位，追加商品/SKU/扩展修订）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `sku_id` - 卡券类目对应的 VOUCHER SKU 稳定 ID
/// * `req` - 更新请求（含商品乐观锁版本）
///
/// # 返回
/// 返回新建卡券类目扩展修订的响应视图。
pub async fn voucher_category_update(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(sku_id): Path<String>,
    Json(req): Json<UpdateVoucherCategoryRequest>,
) -> Result<VoucherCategoryProfileView> {
    let view = CatalogService::new(state.db())
        .voucher_category_update(&sku_id, req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}
