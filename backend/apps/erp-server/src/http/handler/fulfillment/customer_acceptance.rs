//! 客户验收 HTTP 协议适配。

use std::sync::Arc;

use application_core::AuditActor;
use axum::Extension;
use axum::extract::{Path, State};
use erp_fulfillment::dto::{
    CommitCustomerAcceptanceRequest, CreateCustomerAcceptanceRequest, CustomerAcceptanceDetailView,
    CustomerAcceptanceListParams, CustomerAcceptanceView, PageView, PostCustomerAcceptanceRequest,
    ReverseCustomerAcceptanceRequest,
};
use erp_fulfillment_execution::Error as ProcessError;
use erp_fulfillment_execution::fulfillment_execution::customer_acceptance::CustomerAcceptanceProcess;
use erp_fulfillment_execution::workflow::FulfillmentWorkflowServices;
use erp_read_models::fulfillment_center::access::AcceptanceReadService;
use erp_read_models::fulfillment_center::dto::{AcceptanceEligibilityView, CommitCustomerAcceptanceView};

use crate::http::errors::Result;
use crate::http::extractor::{Json, Query};
use crate::http::response::ApiResponse;
use crate::http::state::HttpState;
use crate::wiring::approval_runtime;
use crate::wiring::workflow::work_item_service;

// 为验收草稿和过账命令装配跨域入口，复用真实对象读取授权及 RBAC。
fn acceptance_process(state: &HttpState) -> CustomerAcceptanceProcess {
    CustomerAcceptanceProcess::new(
        state.application().db(),
        state.application().authorization(),
        Arc::new(FulfillmentWorkflowServices::new(
            work_item_service(
                state.application().db(),
                state.application().rbac(),
                state.application().authorization(),
            ),
            approval_runtime(
                state.application().db(),
                state.application().rbac(),
                state.application().authorization(),
            ),
        )),
    )
}

/// 查询客户验收单列表（W06 验收历史视图）。
/// `query` 为分页与筛选参数（`sales_order_id`/`status` 扁平传递）。
pub async fn customer_acceptance_list(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Query(params): Query<CustomerAcceptanceListParams>,
) -> Result<PageView<CustomerAcceptanceView>> {
    let page = AcceptanceReadService::new(state.application().db(), state.application().authorization())
        .list(&params, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(page))
}

/// 查询客户验收单详情（表头 + 行 + 分配）。
/// 返回验收单详情视图。
pub async fn customer_acceptance_detail(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> Result<CustomerAcceptanceDetailView> {
    let view = AcceptanceReadService::new(state.application().db(), state.application().authorization())
        .detail(&id, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

/// 创建客户验收单（草稿；分配在过账时校验并写入）。
/// 返回新建验收单的响应视图。
pub async fn customer_acceptance_create(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<CreateCustomerAcceptanceRequest>,
) -> Result<CustomerAcceptanceView> {
    let view = acceptance_process(&state).create_customer_acceptance(req, &actor).await?;

    Ok(ApiResponse::ok_with_data(view))
}

/// 原子创建或替换客户验收草稿并立即过账。
/// `req` 为最终表头、行、履约分配和乐观锁版本。
/// 返回已过账验收单和过账后的剩余可验收事实。
pub async fn customer_acceptance_commit(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<CommitCustomerAcceptanceRequest>,
) -> Result<CommitCustomerAcceptanceView> {
    let view = acceptance_process(&state).commit_customer_acceptance(req, &actor).await?;

    Ok(ApiResponse::ok_with_data(view))
}

/// 过账客户验收（验收行锁定 + 履约分配守恒 + 净验收上限校验同事务，§8.2 第 5 条）。
/// 返回过账后的验收单视图。
pub async fn customer_acceptance_post(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<PostCustomerAcceptanceRequest>,
) -> Result<CustomerAcceptanceView> {
    let view = acceptance_process(&state).post_customer_acceptance(&id, req, &actor).await?;

    Ok(ApiResponse::ok_with_data(view))
}

/// 冲正客户验收（误录时新增反向验收与反向分配，不覆盖原事实）。
/// `req` 为冲正请求（期望版本 + 原因）。
/// 返回新建反向验收单的视图。
pub async fn customer_acceptance_reverse(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<ReverseCustomerAcceptanceRequest>,
) -> Result<CustomerAcceptanceView> {
    let view = acceptance_process(&state).reverse_customer_acceptance(&id, req, &actor).await?;

    Ok(ApiResponse::ok_with_data(view))
}

/// 查询客户验收工作台（W06：销售行 + 可验收事实 + 验收历史）。
/// `query` 为销售单（`sales_order_id` 必填）。
/// 返回验收工作台视图。
pub async fn customer_acceptance_eligible(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Query(params): Query<CustomerAcceptanceListParams>,
) -> Result<AcceptanceEligibilityView> {
    let sales_order_id = params
        .sales_order_id
        .clone()
        .ok_or_else(|| ProcessError::ValidationError("sales_order_id 不能为空".to_string()))?;
    let view = AcceptanceReadService::new(state.application().db(), state.application().authorization())
        .eligibility(sales_order_id.as_ref(), &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}
