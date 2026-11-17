//! 域 D10 `catalog` 的 HTTP handler（商品字典组）。
//!
//! Handler 只做协议适配：`Validate`（DTO 内联）→ Service 调用 → `ApiResponse`，
//! 直接复用 `erp_catalog` 的 DTO，禁止重复定义同构类型、禁止直连数据库。
//! 本文件覆盖商品分类/品牌/计量单位/规格属性/规格属性值五组字典接口；
//! SPU/SKU 与卡券类目接口见同目录 `product.rs`。

pub mod brand_import;
pub mod delivery_coverage;
pub mod import;
pub mod product;
pub mod source_diff;

use application_core::AuditActor;
use axum::Extension;
use axum::extract::{Multipart, Path, State};
use erp_catalog::{
    CreateProductBrandRequest, CreateProductCategoryRequest, CreateSkuAttributeRequest,
    CreateSkuAttributeValueRequest, CreateUnitOfMeasureRequest, MoveProductCategoryRequest, PageView,
    ProductBrandListParams, ProductBrandView, ProductCategoryListParams, ProductCategoryView,
    SkuAttributeListParams, SkuAttributeValueListParams, SkuAttributeValueView, SkuAttributeView,
    UnitOfMeasureListParams, UnitOfMeasureView, UpdateProductBrandRequest, UpdateProductCategoryRequest,
    UpdateSkuAttributeRequest, UpdateSkuAttributeValueRequest, UpdateUnitOfMeasureRequest,
};
use erp_master_data::{
    create_product_category, create_sku_attribute, create_sku_attribute_value, create_unit_of_measure,
    product_brand_create_with_assets as create_brand_with_assets,
    product_brand_update_with_assets as update_brand_with_assets,
};
use erp_support::SensitivityClass;

use crate::http::errors::Result;
use crate::http::extractor::{Json, Query};
use crate::http::handler::file_asset::{
    extract_command_with_asset_files, finish_asset_command, store_pending_asset_files,
};
use crate::http::response::ApiResponse;
use crate::http::state::HttpState;

/// 查询商品分类列表。
///
/// # 参数
/// * `state` - 应用状态
/// * `query` - 分页与筛选参数（`category_code`/`name`/`parent_category_id`/`status` 扁平传递）
///
/// # 返回
/// 返回契约形状的分页视图（`items`/`total`/`page`/`page_size`）。
pub async fn product_category_list(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Query(params): Query<ProductCategoryListParams>,
) -> Result<PageView<ProductCategoryView>> {
    let page = state.application().catalog_service().product_category_list(&actor, &params).await?;

    Ok(ApiResponse::ok_with_data(page))
}

/// 创建商品分类。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `req` - 创建请求（`{ category_code?, parent_category_id?, name, product_kind, status? }`）；省略代码时自动取号。
///
/// # 返回
/// 返回新建分类的响应视图。
pub async fn product_category_create(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<CreateProductCategoryRequest>,
) -> Result<ProductCategoryView> {
    let view = create_product_category(
        state.application().db(),
        state.application().authorization(),
        state.application().catalog_service(),
        req,
        actor,
    )
    .await?;

    Ok(ApiResponse::ok_with_data(view))
}

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
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<UpdateProductCategoryRequest>,
) -> Result<ProductCategoryView> {
    let view = state.application().catalog_service().product_category_update(&id, req, &actor).await?;

    Ok(ApiResponse::ok_with_data(view))
}

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
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<MoveProductCategoryRequest>,
) -> Result<ProductCategoryView> {
    let view = state.application().catalog_service().product_category_move(&id, req, &actor).await?;

    Ok(ApiResponse::ok_with_data(view))
}

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
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> Result<()> {
    state.application().catalog_service().product_category_delete(&id, &actor).await?;

    Ok(ApiResponse::ok())
}

/// 查询商品品牌列表。
///
/// # 参数
/// * `state` - 应用状态
/// * `query` - 分页与筛选参数（`brand_code`/`name`/`status` 扁平传递）
///
/// # 返回
/// 返回契约形状的分页视图。
pub async fn product_brand_list(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Query(params): Query<ProductBrandListParams>,
) -> Result<PageView<ProductBrandView>> {
    let page = state.application().catalog_service().product_brand_list(&actor, &params).await?;

    Ok(ApiResponse::ok_with_data(page))
}

/// 创建商品品牌。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `req` - 创建请求（`{ brand_code?, name, status? }`）；省略代码时自动取号。
///
/// # 返回
/// 返回新建品牌的响应视图。
pub async fn product_brand_create(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<CreateProductBrandRequest>,
) -> Result<ProductBrandView> {
    let view = state.application().catalog_service().product_brand_create(req, &actor).await?;

    Ok(ApiResponse::ok_with_data(view))
}

/// 一次接收品牌创建命令与 Logo，并原子登记文件元数据和品牌。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 当前认证账号
/// * `multipart` - 品牌 JSON 命令和具名 Logo 文件
///
/// # 返回
/// 返回新建品牌视图。
///
/// # 错误
/// 文件、业务校验或事务失败时返回原错误；提交结果未知时保留对象。
pub async fn product_brand_create_with_assets(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    mut multipart: Multipart,
) -> Result<ProductBrandView> {
    state.application().catalog_service().prepare_brand_create_attachment_upload(&actor).await?;
    let (req, files) = extract_command_with_asset_files::<CreateProductBrandRequest>(&mut multipart).await?;
    let pending = store_pending_asset_files(&state, files, |_| SensitivityClass::General).await?;
    let result =
        create_brand_with_assets(state.application().catalog_service(), req, pending.clone(), actor).await;
    let view = finish_asset_command(&state, &pending, result, |_| true).await?;
    Ok(ApiResponse::ok_with_data(view))
}

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
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<UpdateProductBrandRequest>,
) -> Result<ProductBrandView> {
    let view = state.application().catalog_service().product_brand_update(&id, req, &actor).await?;

    Ok(ApiResponse::ok_with_data(view))
}

/// 一次接收品牌更新命令与 Logo，并原子登记文件元数据和品牌变更。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 当前认证账号
/// * `id` - 品牌 ID
/// * `multipart` - 品牌 JSON 命令和具名 Logo 文件
///
/// # 返回
/// 返回更新后品牌视图。
///
/// # 错误
/// 文件、版本或事务失败时返回原错误；提交结果未知时保留对象。
pub async fn product_brand_update_with_assets(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    mut multipart: Multipart,
) -> Result<ProductBrandView> {
    state.application().catalog_service().prepare_brand_update_attachment_upload(&actor).await?;
    let (req, files) = extract_command_with_asset_files::<UpdateProductBrandRequest>(&mut multipart).await?;
    let pending = store_pending_asset_files(&state, files, |_| SensitivityClass::General).await?;
    let result =
        update_brand_with_assets(state.application().catalog_service(), id, req, pending.clone(), actor)
            .await;
    let view = finish_asset_command(&state, &pending, result, |_| true).await?;
    Ok(ApiResponse::ok_with_data(view))
}

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
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> Result<()> {
    state.application().catalog_service().product_brand_delete(&id, &actor).await?;

    Ok(ApiResponse::ok())
}

/// 查询计量单位列表。
///
/// # 参数
/// * `state` - 应用状态
/// * `query` - 分页与筛选参数（`unit_code`/`name`/`status` 扁平传递）
///
/// # 返回
/// 返回契约形状的分页视图。
pub async fn unit_of_measure_list(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Query(params): Query<UnitOfMeasureListParams>,
) -> Result<PageView<UnitOfMeasureView>> {
    let page = state.application().catalog_service().unit_of_measure_list(&actor, &params).await?;

    Ok(ApiResponse::ok_with_data(page))
}

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
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<CreateUnitOfMeasureRequest>,
) -> Result<UnitOfMeasureView> {
    let view =
        create_unit_of_measure(state.application().db(), state.application().authorization(), req, actor)
            .await?;

    Ok(ApiResponse::ok_with_data(view))
}

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
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<UpdateUnitOfMeasureRequest>,
) -> Result<UnitOfMeasureView> {
    let view = state.application().catalog_service().unit_of_measure_update(&id, req, &actor).await?;

    Ok(ApiResponse::ok_with_data(view))
}

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
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> Result<()> {
    state.application().catalog_service().unit_of_measure_delete(&id, &actor).await?;

    Ok(ApiResponse::ok())
}

/// 查询规格属性列表。
///
/// # 参数
/// * `state` - 应用状态
/// * `query` - 分页与筛选参数（`attribute_code`/`name`/`value_type`/`status` 扁平传递）
///
/// # 返回
/// 返回契约形状的分页视图。
pub async fn sku_attribute_list(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Query(params): Query<SkuAttributeListParams>,
) -> Result<PageView<SkuAttributeView>> {
    let page = state.application().catalog_service().sku_attribute_list(&actor, &params).await?;

    Ok(ApiResponse::ok_with_data(page))
}

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
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<CreateSkuAttributeRequest>,
) -> Result<SkuAttributeView> {
    let view =
        create_sku_attribute(state.application().db(), state.application().authorization(), req, actor)
            .await?;

    Ok(ApiResponse::ok_with_data(view))
}

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
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<UpdateSkuAttributeRequest>,
) -> Result<SkuAttributeView> {
    let view = state.application().catalog_service().sku_attribute_update(&id, req, &actor).await?;

    Ok(ApiResponse::ok_with_data(view))
}

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
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> Result<()> {
    state.application().catalog_service().sku_attribute_delete(&id, &actor).await?;

    Ok(ApiResponse::ok())
}

/// 查询规格属性值列表。
///
/// # 参数
/// * `state` - 应用状态
/// * `query` - 分页与筛选参数（`attribute_id`/`value_code`/`display_value`/`status` 扁平传递）
///
/// # 返回
/// 返回契约形状的分页视图。
pub async fn sku_attribute_value_list(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Query(params): Query<SkuAttributeValueListParams>,
) -> Result<PageView<SkuAttributeValueView>> {
    let page = state.application().catalog_service().sku_attribute_value_list(&actor, &params).await?;

    Ok(ApiResponse::ok_with_data(page))
}

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
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<CreateSkuAttributeValueRequest>,
) -> Result<SkuAttributeValueView> {
    let view = create_sku_attribute_value(
        state.application().db(),
        state.application().authorization(),
        state.application().catalog_service(),
        req,
        actor,
    )
    .await?;

    Ok(ApiResponse::ok_with_data(view))
}

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
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<UpdateSkuAttributeValueRequest>,
) -> Result<SkuAttributeValueView> {
    let view = state.application().catalog_service().sku_attribute_value_update(&id, req, &actor).await?;

    Ok(ApiResponse::ok_with_data(view))
}

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
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> Result<()> {
    state.application().catalog_service().sku_attribute_value_delete(&id, &actor).await?;

    Ok(ApiResponse::ok())
}
