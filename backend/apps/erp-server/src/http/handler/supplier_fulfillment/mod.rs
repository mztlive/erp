//! 域 D32 `supplier_fulfillment` 的 HTTP handler。
//!
//! Handler 只做协议适配：`Validate`（DTO 内联）→ Service 调用 → `ApiResponse`，
//! 直接复用 供应链领域及组合层的 DTO，禁止重复定义同构类型、
//! 禁止直连数据库。Connector 由启动组合根注入，Handler 不选择实现。

use std::sync::Arc;

pub mod execution_setup;

use application_core::AuditActor;
use axum::Extension;
use axum::extract::{Path, State};
use erp_fulfillment_execution::supply_execution::dto::{
    SupplierOrderInvestigationResultView, SupplierOrderTaskCompletionResultView,
};
use erp_read_models::supplier_center::fulfillment_dto::SupplierFulfillmentOrderDetailView;
use erp_supply::dto::supplier_fulfillment::{
    FulfillmentHandoverCandidateView, HandoverFulfillmentOrderRequest, HandoverFulfillmentOrderView,
    PlaceFulfillmentOrderRequest, RecordRefundResultRequest, RecordSupplierRejectRequest,
    SubmitActionResultView, SubmitAfterSalesActionRequest, SupplierFulfillmentOrderDetailParams,
    SupplierFulfillmentOrderListParams, SupplierFulfillmentOrderListView, SupplierFulfillmentOrderView,
    SupplierOrderObjectInvestigationCommand, SupplierOrderStatusHistoryView,
    SupplierOrderTaskCompletionCommand, SupplierOrderTaskInvestigationCommand, SupplierRefundFactView,
};

use crate::http::errors::Result;
use crate::http::extractor::{Json, Query};
use crate::http::response::ApiResponse;
use crate::http::state::HttpState;

/// 按请求条件查询供应商履约订单列表。
/// `query` 为分页与筛选参数（`supplier_id`/三条状态/`external_order_no` 扁平传递）。
pub async fn supplier_fulfillment_order_list(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Query(params): Query<SupplierFulfillmentOrderListParams>,
) -> Result<SupplierFulfillmentOrderListView> {
    let page = state
        .application()
        .supplier_fulfillment_list_read_service()
        .supplier_fulfillment_order_list(&params, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(page))
}

/// 查询供应商履约订单详情，将认证操作人与路径对象交给读模型校验。
/// 返回订单详情视图（订单 + 明细 + 状态历史 + 动作 + 退款事实）。
pub async fn supplier_fulfillment_order_detail(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Query(params): Query<SupplierFulfillmentOrderDetailParams>,
) -> Result<SupplierFulfillmentOrderDetailView> {
    let view = erp_read_models::supplier_center::SupplierFulfillmentDetailReadService::new(
        state.application().db(),
        state.application().supplier_fulfillment_service(),
        state.application().authorization(),
    )
    .supplier_fulfillment_order_detail(
        &id,
        &params,
        &actor,
        Arc::new(state.application().work_item_authorization()),
    )
    .await?;

    Ok(ApiResponse::ok_with_data(view))
}

/// 普通供应商订单结果待核实时走订单调查流程；只有证据确认可安全重放时才执行重放。
pub async fn supplier_fulfillment_order_investigation(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Json(command): Json<SupplierOrderObjectInvestigationCommand>,
) -> Result<SupplierOrderInvestigationResultView> {
    let view = state.application().supplier_fulfillment_process().investigate_order(command, &actor).await?;

    Ok(ApiResponse::ok_with_data(view))
}

/// 在 W26 正式任务上下文核实供应商订单原结果；原结果未定时也只执行已证明安全的重放。
pub async fn supplier_fulfillment_order_task_investigation(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Json(command): Json<SupplierOrderTaskInvestigationCommand>,
) -> Result<SupplierOrderInvestigationResultView> {
    let view =
        state.application().supplier_fulfillment_process().investigate_order_task(command, &actor).await?;

    Ok(ApiResponse::ok_with_data(view))
}

/// 在 W26 正式任务结案时提交完成命令；流程只用服务端可验证的终态证据判定是否能完成。
pub async fn supplier_fulfillment_order_task_completion(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Json(command): Json<SupplierOrderTaskCompletionCommand>,
) -> Result<SupplierOrderTaskCompletionResultView> {
    let view =
        state.application().supplier_fulfillment_process().complete_order_task(command, &actor).await?;

    Ok(ApiResponse::ok_with_data(view))
}

/// 向供应商提交下单（幂等键：`fulfillment_order_no`）。
/// 返回下单后订单的响应视图。
pub async fn supplier_fulfillment_order_submit(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<PlaceFulfillmentOrderRequest>,
) -> Result<SupplierFulfillmentOrderView> {
    let view = state.application().supplier_fulfillment_process().submit_place(req, &actor).await?;

    Ok(ApiResponse::ok_with_data(view))
}

/// 向供应商提交取消（幂等键：「订单号 + CANCEL + 售后申请 ID」）。
/// 返回动作与动作后订单视图。
pub async fn supplier_fulfillment_order_cancel(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<SubmitAfterSalesActionRequest>,
) -> Result<SubmitActionResultView> {
    if let Some(process) = state.application().supplier_api_order_process()
        && let Some(view) = process
            .submit_cancel_if_api(
                &id,
                req.clone(),
                &actor,
                &state.application().supplier_fulfillment_service().access(),
            )
            .await?
    {
        return Ok(ApiResponse::ok_with_data(view));
    }
    let view = state.application().supplier_fulfillment_process().submit_cancel(&id, req, &actor).await?;

    Ok(ApiResponse::ok_with_data(view))
}

/// 向供应商提交退款（幂等键：「订单号 + REFUND + 售后申请 ID」）。
/// 返回动作与动作后订单视图。
pub async fn supplier_fulfillment_order_refund(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<SubmitAfterSalesActionRequest>,
) -> Result<SubmitActionResultView> {
    if let Some(process) = state.application().supplier_after_sales_process()
        && let Some(view) = process
            .submit_refund_if_api(
                &id,
                req.clone(),
                &actor,
                &state.application().supplier_fulfillment_service().access(),
            )
            .await?
    {
        return Ok(ApiResponse::ok_with_data(view));
    }
    let view = state.application().supplier_fulfillment_process().submit_refund(&id, req, &actor).await?;

    Ok(ApiResponse::ok_with_data(view))
}

/// 登记供应商拒单结果（回调幂等键 `(connection_id, external_event_id)`）。
/// 返回新增的状态历史视图。
pub async fn supplier_fulfillment_order_reject(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<RecordSupplierRejectRequest>,
) -> Result<SupplierOrderStatusHistoryView> {
    let view = state.application().supplier_fulfillment_process().record_reject(&id, req, &actor).await?;

    Ok(ApiResponse::ok_with_data(view))
}

/// 登记供应商退款成功结果（幂等键 `(connection_id, external_refund_no,
/// external_refund_version)`）。
/// 返回退款事实视图（含分配行）。
pub async fn supplier_refund_fact_post(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<RecordRefundResultRequest>,
) -> Result<SupplierRefundFactView> {
    if let Some(process) = state.application().supplier_after_sales_process()
        && let Some(view) = process
            .record_refund_if_api(
                &id,
                req.clone(),
                &actor,
                &state.application().supplier_fulfillment_service().access(),
            )
            .await?
    {
        return Ok(ApiResponse::ok_with_data(view));
    }
    let view =
        state.application().supplier_fulfillment_process().record_refund_result(&id, req, &actor).await?;

    Ok(ApiResponse::ok_with_data(view))
}

/// 在内部跟进责任变化时执行显式交接；是否同时转交开放的 W26 任务由此次命令明确指定。
pub async fn supplier_fulfillment_order_handover(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<HandoverFulfillmentOrderRequest>,
) -> Result<HandoverFulfillmentOrderView> {
    let view = state
        .application()
        .supplier_fulfillment_process()
        .handover_fulfillment_order(&id, req, &actor)
        .await?;
    Ok(ApiResponse::ok_with_data(view))
}

/// 开始供应商履约交接前查询该订单可选择的合格内部人员，供交接时选定目标。
pub async fn supplier_fulfillment_order_handover_candidates(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> Result<Vec<FulfillmentHandoverCandidateView>> {
    let view = state.application().supplier_fulfillment_process().handover_candidates(&id, &actor).await?;
    Ok(ApiResponse::ok_with_data(view))
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use config::{Config, SafeConfig};
    use mongodb::Client;
    use mongodb::options::ClientOptions;
    use storage::{S3Storage, S3StorageConfig};
    use tower::ServiceExt;

    use super::*;
    use crate::http::routes;

    // 为供应商履约路由认证单测构造本地 HTTP 状态；请求应在任何数据库或对象存储访问前被拒绝。
    fn state() -> HttpState {
        let settings = Config::from_toml_str(
            r#"
            [xxl_job]
admin_url = "http://admin.invalid/xxl-job-admin"
executor_url = "http://worker.invalid:10002"
app_name = "erp-test"
access_token = "test-token-123456"

[app]
            port=10001
            secret="unit-test-only-secret-at-least-32-bytes"
            [database]
            uri="mongodb://127.0.0.1:1/unit_test"
            db_name="unit_test"
            [s3]
            bucket="unit-test"
            region="unit-test"
            access_key_id="unit-test"
            secret_access_key="unit-test"
            public_base_url="http://127.0.0.1:1"
        "#,
        )
        .unwrap();
        let db = Client::with_options(ClientOptions::default()).unwrap().database("unit_test");
        let storage = S3Storage::new(S3StorageConfig::new(
            "unit-test",
            "unit-test",
            "unit-test",
            "unit-test",
            "http://127.0.0.1:1",
        ))
        .unwrap();
        HttpState::new(db, SafeConfig::new(settings), storage)
    }

    #[tokio::test]
    async fn refund_commands_require_authentication_before_supplier_or_financial_access() {
        let app = routes::create(state());
        for path in [
            "/admin/supplier-fulfillment-orders/order/refund",
            "/admin/supplier-fulfillment-orders/order/refund-facts",
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::post(path)
                        .header("content-type", "application/json")
                        .body(Body::from("{}"))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        }
    }

    #[tokio::test]
    async fn refund_fact_handler_extracts_order_path_before_rejecting_malformed_body() {
        use axum::Router;
        use axum::routing::post;
        use erp_core::AccountKind;
        let app = Router::new()
            .route("/orders/{id}/refund-facts", post(supplier_refund_fact_post))
            .with_state(state())
            .layer(Extension(AuditActor::new("actor".into(), "actor".into(), AccountKind::Admin)));
        let response = app
            .oneshot(
                Request::post("/orders/order/refund-facts")
                    .header("content-type", "application/json")
                    .body(Body::from("{"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }
}
