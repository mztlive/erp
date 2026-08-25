//! 域 D10 `catalog` 的 HTTP handler（商品字典组）。
//!
//! Handler 只做协议适配：`Validate`（DTO 内联）→ Service 调用 → `ApiResponse`，
//! 直接复用 `services::catalog` 的 DTO，禁止重复定义同构类型、禁止直连数据库。
//! 本文件覆盖商品分类/品牌/计量单位/规格属性/规格属性值五组字典接口；
//! SPU/SKU 与卡券类目接口见同目录 `product.rs`。

pub mod product;

use axum::{
    extract::{Multipart, Path, Query, State},
    Extension, Json,
};
use entities::file_asset::SensitivityClass;
use services::audit::AuditActor;
use services::catalog::{
    CatalogService, CreateProductBrandRequest, CreateProductCategoryRequest, CreateSkuAttributeRequest,
    CreateSkuAttributeValueRequest, CreateUnitOfMeasureRequest, MoveProductCategoryRequest, PageView,
    ProductBrandListParams, ProductBrandView, ProductCategoryListParams, ProductCategoryView,
    SkuAttributeListParams, SkuAttributeValueListParams, SkuAttributeValueView, SkuAttributeView,
    UnitOfMeasureListParams, UnitOfMeasureView, UpdateProductBrandRequest, UpdateProductCategoryRequest,
    UpdateSkuAttributeRequest, UpdateSkuAttributeValueRequest, UpdateUnitOfMeasureRequest,
};

use crate::{
    app_state::AppState,
    core::{
        errors::Result,
        handler::file_asset::{
            delete_pending_asset_objects, extract_command_with_asset_files, should_compensate_pending_assets,
            store_pending_asset_files,
        },
        response::ApiResponse,
    },
};

#[permission_macros::permission(
    group = "商品与仓库",
    group_desc = "公司商品池、商品、类目、供应商与仓库基础资料",
    desc = "查询商品分类列表",
    resource = "product_category",
    action = "list"
)]
/// 查询商品分类列表。
///
/// # 参数
/// * `state` - 应用状态
/// * `query` - 分页与筛选参数（`category_code`/`name`/`parent_category_id`/`status` 扁平传递）
///
/// # 返回
/// 返回契约形状的分页视图（`items`/`total`/`page`/`page_size`）。
pub async fn product_category_list(
    State(state): State<AppState>,
    Query(params): Query<ProductCategoryListParams>,
) -> Result<PageView<ProductCategoryView>> {
    let page = CatalogService::new(state.db())
        .product_category_list(&params)
        .await?;

    Ok(ApiResponse::ok_with_data(page))
}

#[permission_macros::permission(
    group = "商品与仓库",
    group_desc = "公司商品池、商品、类目、供应商与仓库基础资料",
    desc = "创建商品分类",
    resource = "product_category",
    action = "create"
)]
/// 创建商品分类。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `req` - 创建请求（`{ category_code, parent_category_id?, name, product_kind, status? }`）
///
/// # 返回
/// 返回新建分类的响应视图。
pub async fn product_category_create(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<CreateProductCategoryRequest>,
) -> Result<ProductCategoryView> {
    let view = CatalogService::new(state.db())
        .product_category_create(req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "商品与仓库",
    group_desc = "公司商品池、商品、类目、供应商与仓库基础资料",
    desc = "更新商品分类",
    resource = "product_category",
    action = "update"
)]
/// 更新商品分类（乐观锁：请求携带期望版本，冲突返回 409）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 分类 ID
/// * `req` - 更新请求（含期望版本）
///
/// # 返回
/// 返回更新后分类的响应视图。
pub async fn product_category_update(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<UpdateProductCategoryRequest>,
) -> Result<ProductCategoryView> {
    let view = CatalogService::new(state.db())
        .product_category_update(&id, req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "商品与仓库",
    group_desc = "公司商品池、商品、类目、供应商与仓库基础资料",
    desc = "移动商品分类",
    resource = "product_category",
    action = "update"
)]
/// 移动商品分类到新父分类（树形维护，成环检测在服务层完成）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 分类 ID
/// * `req` - 移动请求（含期望版本与新父分类）
///
/// # 返回
/// 返回移动后分类的响应视图。
pub async fn product_category_move(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<MoveProductCategoryRequest>,
) -> Result<ProductCategoryView> {
    let view = CatalogService::new(state.db())
        .product_category_move(&id, req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "商品与仓库",
    group_desc = "公司商品池、商品、类目、供应商与仓库基础资料",
    desc = "删除商品分类",
    resource = "product_category",
    action = "delete"
)]
/// 删除商品分类（软删除；存在子分类时拒绝）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 分类 ID
///
/// # 返回
/// 返回删除结果（`data` 为 `null`）。
pub async fn product_category_delete(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> Result<()> {
    CatalogService::new(state.db())
        .product_category_delete(&id, &actor)
        .await?;

    Ok(ApiResponse::ok())
}

#[permission_macros::permission(
    group = "商品与仓库",
    group_desc = "公司商品池、商品、类目、供应商与仓库基础资料",
    desc = "查询商品品牌列表",
    resource = "product_brand",
    action = "list"
)]
/// 查询商品品牌列表。
///
/// # 参数
/// * `state` - 应用状态
/// * `query` - 分页与筛选参数（`brand_code`/`name`/`status` 扁平传递）
///
/// # 返回
/// 返回契约形状的分页视图。
pub async fn product_brand_list(
    State(state): State<AppState>,
    Query(params): Query<ProductBrandListParams>,
) -> Result<PageView<ProductBrandView>> {
    let page = CatalogService::new(state.db())
        .product_brand_list(&params)
        .await?;

    Ok(ApiResponse::ok_with_data(page))
}

#[permission_macros::permission(
    group = "商品与仓库",
    group_desc = "公司商品池、商品、类目、供应商与仓库基础资料",
    desc = "创建商品品牌",
    resource = "product_brand",
    action = "create"
)]
/// 创建商品品牌。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `req` - 创建请求（`{ brand_code, name, status? }`）
///
/// # 返回
/// 返回新建品牌的响应视图。
pub async fn product_brand_create(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<CreateProductBrandRequest>,
) -> Result<ProductBrandView> {
    let view = CatalogService::new(state.db())
        .product_brand_create(req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "商品与仓库",
    group_desc = "公司商品池、商品、类目、供应商与仓库基础资料",
    desc = "一次创建商品品牌及Logo",
    resource = "product_brand",
    action = "create"
)]
/// 一次接收品牌创建命令与 Logo，并原子登记文件元数据和品牌。
pub async fn product_brand_create_with_assets(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    mut multipart: Multipart,
) -> Result<ProductBrandView> {
    let (req, files) = extract_command_with_asset_files::<CreateProductBrandRequest>(&mut multipart).await?;
    let pending = store_pending_asset_files(&state, files, |_| SensitivityClass::General).await?;
    let result = CatalogService::new(state.db())
        .product_brand_create_with_assets(req, pending.clone(), &actor)
        .await;
    match result {
        Ok(view) => Ok(ApiResponse::ok_with_data(view)),
        Err(error) => {
            if should_compensate_pending_assets(&error) {
                delete_pending_asset_objects(&state, &pending).await;
            }
            Err(error.into())
        }
    }
}

#[permission_macros::permission(
    group = "商品与仓库",
    group_desc = "公司商品池、商品、类目、供应商与仓库基础资料",
    desc = "更新商品品牌",
    resource = "product_brand",
    action = "update"
)]
/// 更新商品品牌（乐观锁）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 品牌 ID
/// * `req` - 更新请求（含期望版本）
///
/// # 返回
/// 返回更新后品牌的响应视图。
pub async fn product_brand_update(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<UpdateProductBrandRequest>,
) -> Result<ProductBrandView> {
    let view = CatalogService::new(state.db())
        .product_brand_update(&id, req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "商品与仓库",
    group_desc = "公司商品池、商品、类目、供应商与仓库基础资料",
    desc = "一次更新商品品牌及Logo",
    resource = "product_brand",
    action = "update"
)]
/// 一次接收品牌更新命令与 Logo，并原子登记文件元数据和品牌变更。
pub async fn product_brand_update_with_assets(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    mut multipart: Multipart,
) -> Result<ProductBrandView> {
    let (req, files) = extract_command_with_asset_files::<UpdateProductBrandRequest>(&mut multipart).await?;
    let pending = store_pending_asset_files(&state, files, |_| SensitivityClass::General).await?;
    let result = CatalogService::new(state.db())
        .product_brand_update_with_assets(&id, req, pending.clone(), &actor)
        .await;
    match result {
        Ok(view) => Ok(ApiResponse::ok_with_data(view)),
        Err(error) => {
            if should_compensate_pending_assets(&error) {
                delete_pending_asset_objects(&state, &pending).await;
            }
            Err(error.into())
        }
    }
}

#[permission_macros::permission(
    group = "商品与仓库",
    group_desc = "公司商品池、商品、类目、供应商与仓库基础资料",
    desc = "删除商品品牌",
    resource = "product_brand",
    action = "delete"
)]
/// 删除商品品牌（软删除）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 品牌 ID
///
/// # 返回
/// 返回删除结果（`data` 为 `null`）。
pub async fn product_brand_delete(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> Result<()> {
    CatalogService::new(state.db())
        .product_brand_delete(&id, &actor)
        .await?;

    Ok(ApiResponse::ok())
}

#[permission_macros::permission(
    group = "商品与仓库",
    group_desc = "公司商品池、商品、类目、供应商与仓库基础资料",
    desc = "查询计量单位列表",
    resource = "unit_of_measure",
    action = "list"
)]
/// 查询计量单位列表。
///
/// # 参数
/// * `state` - 应用状态
/// * `query` - 分页与筛选参数（`unit_code`/`name`/`status` 扁平传递）
///
/// # 返回
/// 返回契约形状的分页视图。
pub async fn unit_of_measure_list(
    State(state): State<AppState>,
    Query(params): Query<UnitOfMeasureListParams>,
) -> Result<PageView<UnitOfMeasureView>> {
    let page = CatalogService::new(state.db())
        .unit_of_measure_list(&params)
        .await?;

    Ok(ApiResponse::ok_with_data(page))
}

#[permission_macros::permission(
    group = "商品与仓库",
    group_desc = "公司商品池、商品、类目、供应商与仓库基础资料",
    desc = "创建计量单位",
    resource = "unit_of_measure",
    action = "create"
)]
/// 创建计量单位。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `req` - 创建请求（`{ unit_code, name, symbol, quantity_scale, status? }`）
///
/// # 返回
/// 返回新建单位的响应视图。
pub async fn unit_of_measure_create(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<CreateUnitOfMeasureRequest>,
) -> Result<UnitOfMeasureView> {
    let view = CatalogService::new(state.db())
        .unit_of_measure_create(req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "商品与仓库",
    group_desc = "公司商品池、商品、类目、供应商与仓库基础资料",
    desc = "更新计量单位",
    resource = "unit_of_measure",
    action = "update"
)]
/// 更新计量单位（乐观锁）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 单位 ID
/// * `req` - 更新请求（含期望版本）
///
/// # 返回
/// 返回更新后单位的响应视图。
pub async fn unit_of_measure_update(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<UpdateUnitOfMeasureRequest>,
) -> Result<UnitOfMeasureView> {
    let view = CatalogService::new(state.db())
        .unit_of_measure_update(&id, req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "商品与仓库",
    group_desc = "公司商品池、商品、类目、供应商与仓库基础资料",
    desc = "删除计量单位",
    resource = "unit_of_measure",
    action = "delete"
)]
/// 删除计量单位（软删除）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 单位 ID
///
/// # 返回
/// 返回删除结果（`data` 为 `null`）。
pub async fn unit_of_measure_delete(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> Result<()> {
    CatalogService::new(state.db())
        .unit_of_measure_delete(&id, &actor)
        .await?;

    Ok(ApiResponse::ok())
}

#[permission_macros::permission(
    group = "商品与仓库",
    group_desc = "公司商品池、商品、类目、供应商与仓库基础资料",
    desc = "查询规格属性列表",
    resource = "sku_attribute",
    action = "list"
)]
/// 查询规格属性列表。
///
/// # 参数
/// * `state` - 应用状态
/// * `query` - 分页与筛选参数（`attribute_code`/`name`/`value_type`/`status` 扁平传递）
///
/// # 返回
/// 返回契约形状的分页视图。
pub async fn sku_attribute_list(
    State(state): State<AppState>,
    Query(params): Query<SkuAttributeListParams>,
) -> Result<PageView<SkuAttributeView>> {
    let page = CatalogService::new(state.db())
        .sku_attribute_list(&params)
        .await?;

    Ok(ApiResponse::ok_with_data(page))
}

#[permission_macros::permission(
    group = "商品与仓库",
    group_desc = "公司商品池、商品、类目、供应商与仓库基础资料",
    desc = "创建规格属性",
    resource = "sku_attribute",
    action = "create"
)]
/// 创建规格属性。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `req` - 创建请求（`{ attribute_code, name, value_type, status? }`）
///
/// # 返回
/// 返回新建属性的响应视图。
pub async fn sku_attribute_create(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<CreateSkuAttributeRequest>,
) -> Result<SkuAttributeView> {
    let view = CatalogService::new(state.db())
        .sku_attribute_create(req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "商品与仓库",
    group_desc = "公司商品池、商品、类目、供应商与仓库基础资料",
    desc = "更新规格属性",
    resource = "sku_attribute",
    action = "update"
)]
/// 更新规格属性（乐观锁）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 属性 ID
/// * `req` - 更新请求（含期望版本）
///
/// # 返回
/// 返回更新后属性的响应视图。
pub async fn sku_attribute_update(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<UpdateSkuAttributeRequest>,
) -> Result<SkuAttributeView> {
    let view = CatalogService::new(state.db())
        .sku_attribute_update(&id, req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "商品与仓库",
    group_desc = "公司商品池、商品、类目、供应商与仓库基础资料",
    desc = "删除规格属性",
    resource = "sku_attribute",
    action = "delete"
)]
/// 删除规格属性（软删除）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 属性 ID
///
/// # 返回
/// 返回删除结果（`data` 为 `null`）。
pub async fn sku_attribute_delete(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> Result<()> {
    CatalogService::new(state.db())
        .sku_attribute_delete(&id, &actor)
        .await?;

    Ok(ApiResponse::ok())
}

#[permission_macros::permission(
    group = "商品与仓库",
    group_desc = "公司商品池、商品、类目、供应商与仓库基础资料",
    desc = "查询规格属性值列表",
    resource = "sku_attribute_value",
    action = "list"
)]
/// 查询规格属性值列表。
///
/// # 参数
/// * `state` - 应用状态
/// * `query` - 分页与筛选参数（`attribute_id`/`value_code`/`display_value`/`status` 扁平传递）
///
/// # 返回
/// 返回契约形状的分页视图。
pub async fn sku_attribute_value_list(
    State(state): State<AppState>,
    Query(params): Query<SkuAttributeValueListParams>,
) -> Result<PageView<SkuAttributeValueView>> {
    let page = CatalogService::new(state.db())
        .sku_attribute_value_list(&params)
        .await?;

    Ok(ApiResponse::ok_with_data(page))
}

#[permission_macros::permission(
    group = "商品与仓库",
    group_desc = "公司商品池、商品、类目、供应商与仓库基础资料",
    desc = "创建规格属性值",
    resource = "sku_attribute_value",
    action = "create"
)]
/// 创建规格属性值。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `req` - 创建请求（`{ attribute_id, value_code, display_value, sort_order, status? }`）
///
/// # 返回
/// 返回新建属性值的响应视图。
pub async fn sku_attribute_value_create(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<CreateSkuAttributeValueRequest>,
) -> Result<SkuAttributeValueView> {
    let view = CatalogService::new(state.db())
        .sku_attribute_value_create(req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "商品与仓库",
    group_desc = "公司商品池、商品、类目、供应商与仓库基础资料",
    desc = "更新规格属性值",
    resource = "sku_attribute_value",
    action = "update"
)]
/// 更新规格属性值（乐观锁）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 属性值 ID
/// * `req` - 更新请求（含期望版本）
///
/// # 返回
/// 返回更新后属性值的响应视图。
pub async fn sku_attribute_value_update(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<UpdateSkuAttributeValueRequest>,
) -> Result<SkuAttributeValueView> {
    let view = CatalogService::new(state.db())
        .sku_attribute_value_update(&id, req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "商品与仓库",
    group_desc = "公司商品池、商品、类目、供应商与仓库基础资料",
    desc = "删除规格属性值",
    resource = "sku_attribute_value",
    action = "delete"
)]
/// 删除规格属性值（软删除）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 属性值 ID
///
/// # 返回
/// 返回删除结果（`data` 为 `null`）。
pub async fn sku_attribute_value_delete(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> Result<()> {
    CatalogService::new(state.db())
        .sku_attribute_value_delete(&id, &actor)
        .await?;

    Ok(ApiResponse::ok())
}
