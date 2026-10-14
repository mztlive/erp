//! 销售录入收货地址时的配送提示 Handler。

use axum::Json;
use axum::extract::State;
use erp_read_models::catalog_center::delivery_coverage::{
    DeliveryCoverageRequest, DeliveryCoverageView, sellable_delivery_coverage,
};

use crate::app_state::AppState;
use crate::core::errors::Result;
use crate::core::response::ApiResponse;

#[permission_macros::permission(
    group = "商品与仓库",
    group_desc = "公司商品池、商品、类目、供应商与仓库基础资料",
    desc = "查询商品收货地区配送提示",
    resource = "sellable_sku",
    action = "list"
)]
/// 按商品全部有效供给的配送模板，提示收货地区是否无可配送供给或需另付运费。
///
/// # 参数
/// * `state` - 应用状态
/// * `req` - 商品与收货地区
///
/// # 返回
/// 返回逐商品、地区的提示，不含供应商信息。
pub async fn delivery_coverage(
    State(state): State<AppState>,
    Json(req): Json<DeliveryCoverageRequest>,
) -> Result<DeliveryCoverageView> {
    let view = sellable_delivery_coverage(&state.db(), req).await?;
    Ok(ApiResponse::ok_with_data(view))
}
