//! 域 D10 `catalog` 的 HTTP handler（SPU/SKU 与卡券类目组）。
//!
//! Handler 只做协议适配：`Validate`（DTO 内联）→ Service 调用 → `ApiResponse`，
//! 直接复用 `erp_catalog` 的 DTO，禁止重复定义同构类型、禁止直连数据库。
//! 商品字典接口见同目录 `mod.rs`。

use application_core::AuditActor;
use axum::Extension;
use axum::extract::{Multipart, Path, State};
use erp_catalog::{
    CreateProductRequest, CreateVoucherCategoryRequest, DisableProductRequest, HandoverCandidateView,
    HandoverProductRequest, HandoverProductView, PageView, ProductListParams, ProductListView,
    ProductListingView, ProductRevisionListParams, ProductRevisionView, ProductView, SellableSkuListParams,
    SellableSkuView, SkuListParams, SkuRevisionListParams, SkuRevisionView, SkuView,
    UpdateProductListingRequest, UpdateProductRequest, UpdateSkuListingRequest, UpdateVoucherCategoryRequest,
    VoucherCategoryProfileListParams, VoucherCategoryProfileView,
};
use erp_master_data::product_handover::CatalogHandoverPorts;
use erp_master_data::{
    handover_product, product_create_with_assets as create_product_with_assets,
    product_handover_candidates as load_product_handover_candidates,
    product_update_with_assets as update_product_with_assets,
};
use erp_support::SensitivityClass;
use serde::Deserialize;

use crate::http::errors::Result;
use crate::http::extractor::{Json, Query};
use crate::http::handler::file_asset::{
    extract_command_with_asset_files, finish_asset_command, store_pending_asset_files,
};
use crate::http::response::ApiResponse;
use crate::http::state::HttpState;
use crate::wiring::catalog_access;

/// 查询符合销售资格的公司 SKU 只读投影。
/// 返回公司商品池分页视图，不包含采购成本与供应商身份。
pub async fn sellable_sku_list(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Query(params): Query<SellableSkuListParams>,
) -> Result<PageView<SellableSkuView>> {
    let view = state.application().catalog_center().sellable_sku_list(&params, &actor).await?;
    Ok(ApiResponse::ok_with_data(view))
}

/// 按请求条件查询商品（SPU）列表。
pub async fn product_list(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Query(params): Query<ProductListParams>,
) -> Result<ProductListView> {
    let page = state.application().catalog_center().product_list(&params, &actor).await?;

    Ok(ApiResponse::ok_with_data(page))
}

/// 创建商品（SPU + 首个商品修订 + 媒体 + 全部 SKU 行，跨集合事务）。
/// 返回新建商品的响应视图。
pub async fn product_create(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<CreateProductRequest>,
) -> Result<ProductView> {
    let view = state.application().catalog_service().product_create(req, &actor).await?;

    Ok(ApiResponse::ok_with_data(view))
}

/// 一次接收商品创建命令与新增媒体，并原子登记文件元数据和商品聚合。
///
/// 对象存储不具备 MongoDB 事务能力；数据库事务失败时删除本请求刚写入的
/// 对象作为补偿；提交结果未知时保留对象。
/// 返回新建商品视图。
/// 文件校验、权限、业务校验或事务失败时返回原分类错误。
pub async fn product_create_with_assets(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    mut multipart: Multipart,
) -> Result<ProductView> {
    state.application().catalog_service().prepare_create_attachment_upload(&actor).await?;
    let (req, files) = extract_command_with_asset_files::<CreateProductRequest>(&mut multipart).await?;
    let pending = store_pending_asset_files(&state, files, |_| SensitivityClass::General).await?;
    let result =
        create_product_with_assets(state.application().catalog_service(), req, pending.clone(), actor).await;
    let view = finish_asset_command(&state, &pending, result, |_| true).await?;
    Ok(ApiResponse::ok_with_data(view))
}

/// 规格编辑商品（按规范化签名分类保留/新增/重新启用/移除，跨集合事务）。
/// `req` 为规格编辑请求（含期望版本与修订后全部 SKU 行）。
/// 返回编辑后商品的响应视图。
pub async fn product_update(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<UpdateProductRequest>,
) -> Result<ProductView> {
    let view = state.application().catalog_service().product_update(&id, req, &actor).await?;

    Ok(ApiResponse::ok_with_data(view))
}

/// 一次接收商品修订命令与新增媒体，并原子登记文件元数据和商品新修订。
///
/// 对象存储不具备 MongoDB 事务能力；数据库事务失败时删除本请求刚写入的
/// 对象作为补偿；提交结果未知时保留对象。
/// 返回修订后的商品视图。
/// 文件校验、权限、版本或事务失败时返回原分类错误。
pub async fn product_update_with_assets(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    mut multipart: Multipart,
) -> Result<ProductView> {
    state.application().catalog_service().prepare_update_attachment_upload(&actor).await?;
    let (req, files) = extract_command_with_asset_files::<UpdateProductRequest>(&mut multipart).await?;
    let pending = store_pending_asset_files(&state, files, |_| SensitivityClass::General).await?;
    let result =
        update_product_with_assets(state.application().catalog_service(), id, req, pending.clone(), actor)
            .await;
    let view = finish_asset_command(&state, &pending, result, |_| true).await?;
    Ok(ApiResponse::ok_with_data(view))
}

/// 停用商品；当前修订和媒体由后端读取并在跨集合事务内复制。
/// `req` 为停用意图（已见版本、原因和生效日）。
/// 返回停用后的商品视图。
pub async fn product_disable(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<DisableProductRequest>,
) -> Result<ProductView> {
    let view = state.application().catalog_service().product_disable(&id, req, &actor).await?;

    Ok(ApiResponse::ok_with_data(view))
}

/// 一次切换商品下全部当前启用 SKU 的上架状态。
/// 返回 SPU 从当前启用 SKU 继承得到的上架状态与数量。
pub async fn product_listing_update(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<UpdateProductListingRequest>,
) -> Result<ProductListingView> {
    let view = state.application().catalog_service().product_listing_update(&id, req, &actor).await?;
    Ok(ApiResponse::ok_with_data(view))
}

/// 按请求条件查询商品修订列表。
/// `query` 为分页与筛选参数（`product_id`/`status` 扁平传递）。
pub async fn product_revision_list(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Query(params): Query<ProductRevisionListParams>,
) -> Result<PageView<ProductRevisionView>> {
    let page = state.application().catalog_service().product_revision_list(&actor, &params).await?;

    Ok(ApiResponse::ok_with_data(page))
}

/// 按请求条件查询 SKU 列表。
/// `query` 为分页与筛选参数（`sku_no`/`product_id`/`status`/`listing_status` 扁平传递）。
pub async fn sku_list(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Query(params): Query<SkuListParams>,
) -> Result<PageView<SkuView>> {
    let page = state.application().catalog_service().sku_list(&actor, &params).await?;

    Ok(ApiResponse::ok_with_data(page))
}

/// 切换单个 SKU 的上架状态。
/// `req` 为期望版本与目标上架状态。
/// 返回更新后的 SKU 视图。
pub async fn sku_listing_update(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<UpdateSkuListingRequest>,
) -> Result<SkuView> {
    let view = state.application().catalog_service().sku_listing_update(&id, req, &actor).await?;
    Ok(ApiResponse::ok_with_data(view))
}

/// 按请求条件查询 SKU 修订列表。
/// `query` 为分页与筛选参数（`sku_id`/`name`/`barcode`/`status` 扁平传递）。
pub async fn sku_revision_list(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Query(params): Query<SkuRevisionListParams>,
) -> Result<PageView<SkuRevisionView>> {
    let page = state.application().catalog_service().sku_revision_list(&actor, &params).await?;

    Ok(ApiResponse::ok_with_data(page))
}

/// 按请求条件查询卡券类目扩展修订列表。
/// `query` 为分页与筛选参数（`sku_id`/`status` 扁平传递）。
pub async fn voucher_category_profile_list(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Query(params): Query<VoucherCategoryProfileListParams>,
) -> Result<PageView<VoucherCategoryProfileView>> {
    let page = state.application().catalog_service().voucher_category_profile_list(&actor, &params).await?;

    Ok(ApiResponse::ok_with_data(page))
}

/// 原子创建卡券类目（商品 + 首个修订 + 唯一 SKU + [可选内联新建分类] + 卡券类目
/// 扩展修订，跨集合事务）。
/// `req` 为创建请求（`voucher_no` 同时作为 `product_no`/`sku_no` `category_id` 与 `new_category` 二选一）。
/// 返回新建卡券类目扩展修订的响应视图。
pub async fn voucher_category_create(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<CreateVoucherCategoryRequest>,
) -> Result<VoucherCategoryProfileView> {
    let view = state.application().catalog_service().voucher_category_create(req, &actor).await?;

    Ok(ApiResponse::ok_with_data(view))
}

/// 更新卡券类目名称与描述（按 SKU 稳定身份定位，追加商品/SKU/扩展修订）。
/// `req` 为更新请求（含商品乐观锁版本）。
/// 返回新建卡券类目扩展修订的响应视图。
pub async fn voucher_category_update(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(sku_id): Path<String>,
    Json(req): Json<UpdateVoucherCategoryRequest>,
) -> Result<VoucherCategoryProfileView> {
    let view = state.application().catalog_service().voucher_category_update(&sku_id, req, &actor).await?;

    Ok(ApiResponse::ok_with_data(view))
}

/// 商品详情 SKU 修订查询参数（可选过滤到单个 SKU）。
#[derive(Debug, Deserialize)]
pub struct ProductDetailSkuRevisionQuery {
    /// 稳定 SKU ID；缺省返回该商品下全部 SKU 的修订。
    pub sku_id: Option<String>,
}

/// 按稳定 ID 查询单个商品，与列表共用同一聚合计数口径。
/// 返回与列表同构的单个商品视图。
pub async fn product_detail(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> Result<ProductView> {
    let view = state.application().catalog_center().product_detail(&id, &actor).await?;

    Ok(ApiResponse::ok_with_data(view))
}

/// 显式交接商品维护人与可选业务组织。
/// `req` 为交接请求（目标、原因、版本与幂等键）。
/// 返回交接后的维护人、组织与版本。
pub async fn product_handover(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<HandoverProductRequest>,
) -> Result<HandoverProductView> {
    let view = handover_product(
        state.application().db(),
        state.application().authorization(),
        CatalogHandoverPorts::new(
            catalog_access(state.application().db(), state.application().authorization()),
            state.application().catalog_service(),
        ),
        &id,
        req,
        &actor,
    )
    .await?;
    Ok(ApiResponse::ok_with_data(view))
}

/// 查询合格有效的商品维护人交接候选。
/// 返回有效且具备 `product:update` 的账号。
pub async fn product_handover_candidates(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> Result<Vec<HandoverCandidateView>> {
    let view = load_product_handover_candidates(
        state.application().db(),
        state.application().authorization(),
        CatalogHandoverPorts::new(
            catalog_access(state.application().db(), state.application().authorization()),
            state.application().catalog_service(),
        ),
        &id,
        &actor,
    )
    .await?;
    Ok(ApiResponse::ok_with_data(view))
}

/// 查询指定商品的全部修订（强制路径商品过滤）。
/// 返回该商品全部修订分页视图。
pub async fn product_detail_revisions(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> Result<PageView<ProductRevisionView>> {
    let page = state.application().catalog_service().product_detail_revisions(&id, &actor).await?;

    Ok(ApiResponse::ok_with_data(page))
}

/// 查询指定商品的全部 SKU（强制路径商品过滤）。
/// 返回该商品全部 SKU 分页视图。
pub async fn product_detail_skus(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> Result<PageView<SkuView>> {
    let page = state.application().catalog_service().product_detail_skus(&id, &actor).await?;

    Ok(ApiResponse::ok_with_data(page))
}

/// 查询指定商品下的 SKU 修订（可选过滤到单个 SKU）。
/// `query` 为可选 SKU 过滤。
/// 返回该商品下 SKU 修订分页视图。
pub async fn product_detail_sku_revisions(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Query(query): Query<ProductDetailSkuRevisionQuery>,
) -> Result<PageView<SkuRevisionView>> {
    let page = state
        .application()
        .catalog_service()
        .product_detail_sku_revisions(&id, query.sku_id.as_deref(), &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(page))
}

#[cfg(test)]
mod tests {
    use axum::http::Uri;
    use erp_catalog::entity::catalog::SellablePriceKind;
    use erp_core::delivery_coverage::DeliveryExtent;

    use super::SellableSkuListParams;
    use crate::http::extractor::Query;

    #[test]
    fn price_filter_query_accepts_one_kind_and_rejects_invalid_or_repeated_values() {
        for (value, kind) in [
            ("dropship", SellablePriceKind::Dropship),
            ("bulk", SellablePriceKind::Bulk),
            ("market", SellablePriceKind::Market),
        ] {
            let uri: Uri = format!(
                "/admin/sellable-skus?price_kind={value}&sales_price_min=10.00&sales_price_max=20.00"
            )
            .parse()
            .unwrap();
            let Query(params) = Query::<SellableSkuListParams>::try_from_uri(&uri).unwrap();
            assert_eq!(params.price_kind, kind);
            assert_eq!(params.sales_price_min.unwrap().to_string(), "10.00");
        }
        for query in [
            "price_kind=unknown",
            "price_kind=bulk&price_kind=market",
            "price_kind=bulk&sales_price_min=oops",
        ] {
            let uri: Uri = format!("/admin/sellable-skus?{query}").parse().unwrap();
            assert!(Query::<SellableSkuListParams>::try_from_uri(&uri).is_err());
        }
    }

    #[test]
    fn delivery_filter_query_parses_region_extent_and_freight_flag() {
        let uri: Uri =
            "/admin/sellable-skus?delivery_region_code=440300&delivery_extent=PARTIAL&include_freight_extra=true"
                .parse()
                .unwrap();
        let Query(params) = Query::<SellableSkuListParams>::try_from_uri(&uri).unwrap();
        assert_eq!(params.delivery_region_code.as_deref(), Some("440300"));
        assert_eq!(params.delivery_extent, Some(DeliveryExtent::Partial));
        assert_eq!(params.include_freight_extra, Some(true));
        for query in ["delivery_extent=SOME", "include_freight_extra=maybe"] {
            let uri: Uri = format!("/admin/sellable-skus?{query}").parse().unwrap();
            assert!(Query::<SellableSkuListParams>::try_from_uri(&uri).is_err());
        }
    }
}
