//! 采购退货 HTTP 协议适配。

use application_core::AuditActor;
use axum::extract::{Path, Query, State};
use axum::{Extension, Json};
use erp_processes::adapters::MongoPurchaseDataScope;
use erp_processes::reverse_flow::ReturnsProcess;
use erp_read_models::returns_center::dto::{PurchaseReturnOrderListParams, PurchaseReturnOrderView};
use erp_read_models::returns_center::{PurchaseReturnListView, ReturnsReadService};
use erp_returns::dto::CreatePurchaseReturnOrderRequest;

use crate::app_state::AppState;
use crate::core::errors::Result;
use crate::core::response::ApiResponse;

#[permission_macros::permission(
    group = "退货退款",
    group_desc = "销售退货/拒收、采购退货与退款冲正管理（W05/W09/W11/W12）",
    desc = "查询采购退货单列表",
    resource = "purchase_return_order",
    action = "list"
)]
/// 查询采购退货单列表。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已认证操作人
/// * `query` - 分页与筛选参数（扁平传递，含跨页 `scope_version`）
///
/// # 返回
/// 返回带范围版本的分页视图。
///
/// # 错误
/// 无动作权限、范围变化、筛选非法或仓储失败时拒绝。
///
/// # 关键业务约束
/// 沿来源采购单责任接入；缺范围返回空集并标记 `no_scope`。
pub async fn purchase_return_order_list(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Query(params): Query<PurchaseReturnOrderListParams>,
) -> Result<PurchaseReturnListView> {
    let page = ReturnsReadService::new(state.db())
        .with_purchase_scope(MongoPurchaseDataScope::shared(state.db(), state.rbac()))
        .purchase_return_order_list(&params, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(page))
}

#[permission_macros::permission(
    group = "退货退款",
    group_desc = "销售退货/拒收、采购退货与退款冲正管理（W05/W09/W11/W12）",
    desc = "查询采购退货单详情",
    resource = "purchase_return_order",
    action = "detail"
)]
/// 查询采购退货单详情（退货单 + 明细行）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已认证操作人
/// * `id` - 退货单 ID
///
/// # 返回
/// 返回完整退货单视图。
///
/// # 错误
/// 无动作权限、不可见或不存在时拒绝。
///
/// # 关键业务约束
/// 列表已授权不能作为详情凭证；沿来源采购单 detail 动作重验。
pub async fn purchase_return_order_detail(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> Result<PurchaseReturnOrderView> {
    let view = ReturnsReadService::new(state.db())
        .with_purchase_scope(MongoPurchaseDataScope::shared(state.db(), state.rbac()))
        .purchase_return_order_detail(&id, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "退货退款",
    group_desc = "销售退货/拒收、采购退货与退款冲正管理（W05/W09/W11/W12）",
    desc = "建立采购退货单",
    resource = "purchase_return_order",
    action = "create"
)]
/// 建立采购退货单与明细行（跨集合事务）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `req` - 创建请求
///
/// # 返回
/// 返回新建退货单视图。
///
/// # 错误
/// 业务处理或数据读取失败时返回对应错误。
pub async fn purchase_return_order_create(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<CreatePurchaseReturnOrderRequest>,
) -> Result<PurchaseReturnOrderView> {
    let view = ReturnsProcess::new(state.db())
        .with_object_read(state.approval_object_read())
        .create_purchase_return_order(req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}
