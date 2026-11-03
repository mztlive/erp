//! 销售录入收货地址时的配送提示 Handler。

use axum::extract::State;
use erp_read_models::catalog_center::delivery_coverage::{
    DeliveryCoverageRequest, DeliveryCoverageView, sellable_delivery_coverage,
};

use crate::http::errors::Result;
use crate::http::extractor::Json;
use crate::http::response::ApiResponse;
use crate::http::state::HttpState;

/// 按商品全部有效供给的配送模板，提示收货地区是否无可配送供给或需另付运费。
///
/// # 参数
/// * `state` - 应用状态
/// * `req` - 商品与收货地区
///
/// # 返回
/// 返回逐商品、地区的提示，不含供应商信息。
pub async fn delivery_coverage(
    State(state): State<HttpState>,
    Json(req): Json<DeliveryCoverageRequest>,
) -> Result<DeliveryCoverageView> {
    let view = sellable_delivery_coverage(&state.application().db(), req).await?;
    Ok(ApiResponse::ok_with_data(view))
}
