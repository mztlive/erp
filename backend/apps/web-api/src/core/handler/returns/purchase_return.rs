//! 采购退货 HTTP 协议适配。

use application_core::AuditActor;
use axum::Extension;
use axum::extract::{Path, State};
use erp_processes::adapters::MongoPurchaseDataScope;
use erp_processes::reverse_flow::ReturnsProcess;
use erp_procurement::entity::purchase_order::ReturnSettlementDecision;
use erp_read_models::returns_center::dto::{PurchaseReturnOrderListParams, PurchaseReturnOrderView};
use erp_read_models::returns_center::{PurchaseReturnListView, ReturnsReadService};
use erp_returns::dto::CreatePurchaseReturnOrderRequest;
use erp_returns::entity::returns::DirectReturnRequest;

use crate::app_state::AppState;
use crate::core::errors::Result;
use crate::core::extractor::{Json, Query};
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

#[permission_macros::permission(
    group = "退货退款",
    group_desc = "销售退货/拒收、采购退货与退款冲正管理（W05/W09/W11/W12）",
    desc = "建立采购退货单",
    resource = "purchase_return_order",
    action = "create"
)]
/// 登记已核实的客户直退，保留原采购和发货记录。
/// # 参数
/// * `state` - 应用状态。
/// * `actor` - 已认证采购经办人。
/// * `id` - 来源采购身份。
/// * `req` - 实际退回凭据和同键请求。
/// # 返回
/// 采购退货单身份。
/// # 错误
/// 权限、版本、数量或凭据不满足时拒绝。
pub async fn purchase_direct_return_confirm(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<DirectReturnRequest>,
) -> Result<String> {
    let result = ReturnsProcess::new(state.db())
        .with_rbac(state.rbac())
        .with_object_read(state.approval_object_read())
        .confirm_direct_return(&id, req, &actor)
        .await?;
    Ok(ApiResponse::ok_with_data(result))
}

#[permission_macros::permission(
    group = "退货退款",
    group_desc = "销售退货/拒收、采购退货与退款冲正管理（W05/W09/W11/W12）",
    desc = "建立采购退货单",
    resource = "purchase_return_order",
    action = "create"
)]
/// 同步已有直退的财务责任交接。
/// # 参数
/// * `state` - 应用状态。
/// * `actor` - 已认证经办人。
/// * `id` - 来源采购身份。
/// # 返回
/// 同步成功。
/// # 错误
/// 无权处理采购或责任配置缺失时拒绝。
pub async fn purchase_return_finance_sync(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(decision): Json<ReturnSettlementDecision>,
) -> Result<()> {
    ReturnsProcess::new(state.db())
        .with_rbac(state.rbac())
        .reconcile_return_finance(&id, decision, &actor)
        .await?;
    Ok(ApiResponse::ok_with_data(()))
}

#[cfg(test)]
mod direct_return_body_tests {
    use erp_returns::entity::returns::DirectReturnRequest;
    use serde_json::json;

    #[test]
    fn settlement_body_requires_supplier_evidence_and_conserves_amounts() {
        use std::str::FromStr;

        use erp_core::money::Amount;

        use super::ReturnSettlementDecision;
        let body = json!({"idempotency_key":"decision", "sales_revision_id":"r2", "target_gross":"1000", "refund_amount":"75", "credit_amount":"100", "supplier_confirmation":"供应商确认退款 75、留抵 100"});
        let target = Amount::from_str("1000").unwrap();
        let excess = Amount::from_str("175").unwrap();
        serde_json::from_value::<ReturnSettlementDecision>(body.clone())
            .unwrap()
            .validate("r2", target, excess)
            .unwrap();
        for (key, value) in [
            ("supplier_confirmation", json!("")),
            ("credit_amount", json!("101")),
            ("sales_revision_id", json!("r1")),
        ] {
            let mut invalid = body.clone();
            invalid[key] = value;
            assert!(
                serde_json::from_value::<ReturnSettlementDecision>(invalid)
                    .unwrap()
                    .validate("r2", target, excess)
                    .is_err()
            );
        }
        let mut invalid = body;
        invalid["owner_user_id"] = json!("cannot-assign-finance");
        assert!(serde_json::from_value::<ReturnSettlementDecision>(invalid).is_err());
    }

    #[test]
    fn actual_receipt_body_requires_version_evidence_and_positive_unique_lines() {
        let body = json!({
            "expected_version": 7,
            "idempotency_key": "12345678-1234-1234-1234-123456789abc",
            "reason": "销售减量",
            "receipt_reference": "供应商收货回单 1",
            "lines": [{ "purchase_line_id": "line-1", "quantity": "27" }]
        });
        let request: DirectReturnRequest = serde_json::from_value(body.clone()).unwrap();
        request.validate().unwrap();
        let mut invalid = body.clone();
        invalid["lines"][0]["quantity"] = json!("0");
        assert!(serde_json::from_value::<DirectReturnRequest>(invalid).unwrap().validate().is_err());
        let mut invalid = body.clone();
        invalid["receipt_reference"] = json!("");
        assert!(serde_json::from_value::<DirectReturnRequest>(invalid).unwrap().validate().is_err());
        let mut invalid = body.clone();
        invalid.as_object_mut().unwrap().remove("expected_version");
        assert!(serde_json::from_value::<DirectReturnRequest>(invalid).is_err());
        let mut invalid = body;
        invalid["supplier_id"] = json!("client-must-not-replace-supplier");
        assert!(serde_json::from_value::<DirectReturnRequest>(invalid).is_err());
    }
}
