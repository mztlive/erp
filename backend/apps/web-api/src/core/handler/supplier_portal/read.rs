//! 门户读接口复用允许列表投影，不调用内部商品目录。
use application_core::PageView;
use axum::Extension;
use axum::extract::{Path, Query, State};
use erp_catalog::ProductKind;
use erp_catalog::portal::{
    CatalogPortalService, CategoryMappingSuggestion, DictionaryCandidate, DictionaryKind,
};
use erp_identity::PortalActor;
use erp_read_models::supplier_center::offering::portal::{
    PortalOfferingParams, PortalOfferingReadService, PortalOfferingView, PortalRevisionView,
};
use erp_read_models::supplier_portal::{PortalListParams, SupplierPortalReadService};
use persistence_core::NoTransaction;
use serde::Deserialize;
use serde_json::{Value, to_value};

use crate::app_state::AppState;
use crate::core::errors::{Error, Result};
use crate::core::response::ApiResponse;

/// 读取当前门户身份的必要展示资料。
/// # 参数
/// 身份由门户中间件逐请求重验。
/// # 返回
/// 当前账号和供应商必要展示资料。
/// # 错误
/// 关联读取失败时拒绝。
pub async fn session(
    State(state): State<AppState>,
    Extension(actor): Extension<PortalActor>,
) -> Result<Value> {
    Ok(ApiResponse::ok_with_data(
        to_value(SupplierPortalReadService::new(state.db()).session(&actor).await?)
            .map_err(|error| Error::Internal(error.to_string()))?,
    ))
}

/// 读取当前供应商自己的供给分页。
/// # 参数
/// 仅允许本供应商结果内的筛选。
/// # 返回
/// 必要资料、价格与可供版本。
/// # 错误
/// 非法分页或读取失败时拒绝。
pub async fn offerings(
    State(state): State<AppState>,
    Extension(actor): Extension<PortalActor>,
    Query(params): Query<PortalOfferingParams>,
) -> Result<PageView<PortalOfferingView>> {
    Ok(ApiResponse::ok_with_data(
        PortalOfferingReadService::new(state.db()).offerings(&actor, &params).await?,
    ))
}

/// 读取自己供给的当前资料。
/// # 参数
/// 精确供给标识和当前验证身份。
/// # 返回
/// 外部供给详情。
/// # 错误
/// 未知或范围外目标统一404。
pub async fn offering(
    State(state): State<AppState>,
    Extension(actor): Extension<PortalActor>,
    Path(id): Path<String>,
) -> Result<Value> {
    let current = PortalOfferingReadService::new(state.db()).offering(&actor, &id).await?;
    let pending = SupplierPortalReadService::new(state.db())
        .pending_offering_applications(&actor, &id, &mut NoTransaction)
        .await?;
    let mut data = to_value(current).map_err(|error| Error::Internal(error.to_string()))?;
    data["pending_applications"] = to_value(pending).map_err(|error| Error::Internal(error.to_string()))?;
    Ok(ApiResponse::ok_with_data(data))
}

/// 读取自己供给的不可变条款历史。
/// # 参数
/// 精确供给标识和验证身份。
/// # 返回
/// 外部允许字段的历史版本。
/// # 错误
/// 未知或范围外目标统一404。
pub async fn revisions(
    State(state): State<AppState>,
    Extension(actor): Extension<PortalActor>,
    Path(id): Path<String>,
) -> Result<Vec<PortalRevisionView>> {
    Ok(ApiResponse::ok_with_data(PortalOfferingReadService::new(state.db()).revisions(&actor, &id).await?))
}

/// 读取本供应商定向开放的精确SKU目录。
/// # 参数
/// 搜索及分页仅收窄开放目录。
/// # 返回
/// 公司商品必要资料，不含销售价格。
/// # 错误
/// 输入或读取失败时拒绝。
pub async fn catalog(
    State(state): State<AppState>,
    Extension(actor): Extension<PortalActor>,
    Query(params): Query<PortalListParams>,
) -> Result<Value> {
    let data = SupplierPortalReadService::new(state.db()).catalog(&actor, &params).await?;
    Ok(ApiResponse::ok_with_data(to_value(data).map_err(|error| Error::Internal(error.to_string()))?))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DictionaryParams {
    pub q: Option<String>,
    pub product_kind: Option<ProductKind>,
}

/// 读取独立字典候选；可选择字典不授予维护权限。
/// # 参数
/// 品牌、分类或单位及必要搜索。
/// # 返回
/// 有效字典的最小候选投影。
/// # 错误
/// 字典类型、搜索或读取失败时拒绝。
pub async fn dictionaries(
    State(state): State<AppState>,
    Extension(_actor): Extension<PortalActor>,
    Path(kind): Path<DictionaryKind>,
    Query(params): Query<DictionaryParams>,
) -> Result<Vec<DictionaryCandidate>> {
    let data = CatalogPortalService::new(state.db(), state.catalog_service())
        .dictionary_candidates(kind, params.q.as_deref(), params.product_kind, &mut NoTransaction)
        .await?;
    Ok(ApiResponse::ok_with_data(data))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CategorySuggestionParams {
    pub original_category_path: String,
    pub product_kind: ProductKind,
}

/// 读取本供应商此前确认的分类映射建议；必须人工再次核对。
/// # 参数
/// 完整原始路径和商品类型，供应商归属仅来自当前门户身份。
/// # 返回
/// 映射建议及当前目标状态，不自动关联或修改任何申请。
/// # 错误
/// 路径非法或映射读取失败时拒绝。
pub async fn category_mapping_suggestion(
    State(state): State<AppState>,
    Extension(actor): Extension<PortalActor>,
    Query(params): Query<CategorySuggestionParams>,
) -> Result<Option<CategoryMappingSuggestion>> {
    let suggestion = CatalogPortalService::new(state.db(), state.catalog_service())
        .category_mapping_suggestion(
            &actor.supplier_id,
            &params.original_category_path,
            params.product_kind,
            &mut NoTransaction,
        )
        .await?;
    Ok(ApiResponse::ok_with_data(suggestion))
}

/// 读取当前供应商自己的四类申请。
/// # 参数
/// 当前身份和分页筛选。
/// # 返回
/// 原稿、对外决定和实际结果的允许列表。
/// # 错误
/// 输入或读取失败时拒绝。
pub async fn applications(
    State(state): State<AppState>,
    Extension(actor): Extension<PortalActor>,
    Query(params): Query<PortalListParams>,
) -> Result<Value> {
    let data = SupplierPortalReadService::new(state.db()).applications(&actor, &params).await?;
    Ok(ApiResponse::ok_with_data(to_value(data).map_err(|error| Error::Internal(error.to_string()))?))
}

/// 读取自己申请的原稿及处理结果。
/// # 参数
/// 当前身份和精确申请标识。
/// # 返回
/// 不包含内部任务备注及规范化映射的详情。
/// # 错误
/// 未知或越权统一404。
pub async fn application(
    State(state): State<AppState>,
    Extension(actor): Extension<PortalActor>,
    Path(id): Path<String>,
) -> Result<Value> {
    let data = SupplierPortalReadService::new(state.db()).application(&actor, &id).await?;
    Ok(ApiResponse::ok_with_data(to_value(data).map_err(|error| Error::Internal(error.to_string()))?))
}

/// 读取自己的当前付款条件和必要采购联系人。
/// # 参数
/// 当前门户身份。
/// # 返回
/// 供应商商务档案的外部展示字段。
/// # 错误
/// 供应商或当前档案读取失败时拒绝。
pub async fn cooperation(
    State(state): State<AppState>,
    Extension(actor): Extension<PortalActor>,
) -> Result<Value> {
    let data = SupplierPortalReadService::new(state.db()).cooperation(&actor).await?;
    Ok(ApiResponse::ok_with_data(to_value(data).map_err(|error| Error::Internal(error.to_string()))?))
}
