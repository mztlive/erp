//! 域 D33 `supplier_settlement` 的 HTTP handler。
//!
//! Handler 只做协议适配：`Validate`（DTO 内联）→ Service 调用 → `ApiResponse`，
//! 直接复用 供应链领域及组合层的 DTO，禁止重复定义同构类型、
//! 禁止直连数据库。

pub mod reviewer_options;

use application_core::AuditActor;
use axum::Extension;
use axum::extract::{Path, State};
use erp_read_models::supplier_center::settlement::SupplierSettlementReadService;
use erp_read_models::supplier_center::settlement::display_dto::{
    SettlementEvidenceDisplayView, SettlementItemDisplayView, SettlementStatementDisplayDetail,
    SettlementStatementDisplayList, SettlementStatementDisplayView,
};
use erp_read_models::supplier_center::settlement::dto::SettlementReviewDecisionResult;
use erp_supply::dto::supplier_settlement::{
    CreateSettlementStatementRequest, HandoverCandidateView, HandoverSettlementRequest,
    HandoverSettlementView, ReassignSettlementDifferenceHandlerRequest,
    ReassignSettlementDifferenceHandlerView, RecordSettlementSourceEvidenceRequest,
    RefreshSettlementStatementRequest, SettlementDifferenceDecisionRequest,
    SettlementDifferenceDecisionResult, SettlementDifferenceEvidenceRequest,
    SettlementDifferenceEvidenceResult, SettlementDraftCommandResult, SettlementPageView,
    SettlementReviewCommand, SubmitSettlementReviewRequest, SubmitSettlementReviewResult,
    SupplierSettlementDifferenceListParams, SupplierSettlementDifferenceView,
    SupplierSettlementItemListParams, SupplierSettlementSourceEvidenceQuery,
    SupplierSettlementSourceEvidenceView, SupplierSettlementStatementListParams, VoidSettlementRequest,
};
use erp_supply_settlement::supply_settlement::SupplierSettlementProcess;

use crate::http::errors::Result;
use crate::http::extractor::{Json, Query};
use crate::http::response::ApiResponse;
use crate::http::state::HttpState;
use crate::wiring::workflow::WorkflowAuth;

fn settlement_process(state: &HttpState) -> SupplierSettlementProcess<WorkflowAuth> {
    crate::wiring::scoped_settlement_process(
        state.application().db(),
        state.application().rbac(),
        state.application().authorization(),
    )
}

/// 查询供应商结算单列表。
///
/// # 参数
/// * `state` - 应用状态
/// * `query` - 分页与筛选参数（`statement_no`/`supplier_id`/`status` 扁平传递）
///
/// # 返回
/// 返回契约形状的分页视图（`items`/`total`/`page`/`page_size`）。
pub async fn supplier_settlement_statement_list(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Query(params): Query<SupplierSettlementStatementListParams>,
) -> Result<SettlementStatementDisplayList> {
    let page =
        SupplierSettlementReadService::new(state.application().db(), state.application().authorization())
            .statement_list(&params, &actor)
            .await?;
    let page =
        SupplierSettlementReadService::new(state.application().db(), state.application().authorization())
            .statement_list_view(page)
            .await?;
    Ok(ApiResponse::ok_with_data(page))
}

/// 查询供应商结算单详情。
///
/// # 参数
/// * `state` - 应用状态
/// * `id` - 结算单 ID
///
/// # 返回
/// 返回结算单详情视图（结算单 + 明细 + 差异）。
pub async fn supplier_settlement_statement_detail(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> Result<SettlementStatementDisplayDetail> {
    let view =
        SupplierSettlementReadService::new(state.application().db(), state.application().authorization())
            .supplier_settlement_statement_detail(&id, &actor)
            .await?;

    Ok(ApiResponse::ok_with_data(view))
}

/// 创建供应商结算草稿（幂等键：`statement_no`）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人（经办人）
/// * `req` - 创建请求
///
/// # 返回
/// 返回新建结算单的响应视图。
pub async fn supplier_settlement_statement_create(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<CreateSettlementStatementRequest>,
) -> Result<SettlementDraftCommandResult<SettlementStatementDisplayView>> {
    let view = settlement_process(&state).create_statement(req, &actor).await?;

    let view =
        SupplierSettlementReadService::new(state.application().db(), state.application().authorization())
            .statement_result(view)
            .await?;
    Ok(ApiResponse::ok_with_data(view))
}

/// 从冻结策略下最新来源证据刷新尚未提交复核的结算试算。
pub async fn supplier_settlement_statement_refresh(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<RefreshSettlementStatementRequest>,
) -> Result<SettlementDraftCommandResult<SettlementStatementDisplayView>> {
    let view = settlement_process(&state).refresh_statement(&id, req, &actor).await?;
    let view =
        SupplierSettlementReadService::new(state.application().db(), state.application().authorization())
            .statement_result(view)
            .await?;
    Ok(ApiResponse::ok_with_data(view))
}

/// 查询创建草稿所需的最新完整来源证据批次，不返回金额明细。
pub async fn supplier_settlement_source_evidence_latest(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Query(query): Query<SupplierSettlementSourceEvidenceQuery>,
) -> Result<SupplierSettlementSourceEvidenceView> {
    let view = crate::wiring::scoped_settlement_service(
        state.application().db(),
        state.application().authorization(),
    )
    .latest_source_evidence(&query, &actor)
    .await?;
    Ok(ApiResponse::ok_with_data(view))
}

/// 逐行核验 D32 正式事实并冻结供应商结算来源证据批次。
pub async fn supplier_settlement_source_evidence_record(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<RecordSettlementSourceEvidenceRequest>,
) -> Result<SupplierSettlementSourceEvidenceView> {
    let view = settlement_process(&state).record_source_evidence(req, &actor).await?;
    Ok(ApiResponse::ok_with_data(view))
}

/// 提交结算复核（乐观锁；存在未解决差异时拒绝）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 结算单 ID
/// * `req` - 提交请求（含期望版本）
///
/// # 返回
/// 返回提交后结算单的响应视图。
pub async fn supplier_settlement_statement_submit_review(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<SubmitSettlementReviewRequest>,
) -> Result<SubmitSettlementReviewResult<SettlementStatementDisplayView>> {
    let view = settlement_process(&state).submit_review(&id, req, &actor).await?;

    let view =
        SupplierSettlementReadService::new(state.application().db(), state.application().authorization())
            .statement_result(view)
            .await?;
    Ok(ApiResponse::ok_with_data(view))
}

/// 执行 `REJECT` 或 `CONFIRM` 正式复核决定。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 结算单 ID
/// * `req` - 携带任务、任务版本、主题版本与结算版本的强类型命令
///
/// # 返回
/// 返回正式决定、任务终态、应付与成本差额引用。
pub async fn supplier_settlement_statement_review_decide(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<SettlementReviewCommand>,
) -> Result<SettlementReviewDecisionResult<SettlementStatementDisplayView>> {
    let view = settlement_process(&state).decide_review(&id, req, &actor).await?;

    let view =
        SupplierSettlementReadService::new(state.application().db(), state.application().authorization())
            .statement_result(view)
            .await?;
    Ok(ApiResponse::ok_with_data(view))
}

/// 作废结算单（乐观锁；已作废为终态）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 结算单 ID
/// * `req` - 作废请求（含期望版本与原因）
///
/// # 返回
/// 返回作废后结算单的响应视图。
pub async fn supplier_settlement_statement_void(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<VoidSettlementRequest>,
) -> Result<SettlementStatementDisplayView> {
    let view = settlement_process(&state).void_statement(&id, req, &actor).await?;

    let view =
        SupplierSettlementReadService::new(state.application().db(), state.application().authorization())
            .statement_result(view)
            .await?;
    Ok(ApiResponse::ok_with_data(view))
}

/// 查询供应商结算明细列表。
///
/// # 参数
/// * `state` - 应用状态
/// * `query` - 分页与筛选参数（`statement_id` 扁平传递）
///
/// # 返回
/// 返回契约形状的分页视图。
pub async fn supplier_settlement_item_list(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Query(params): Query<SupplierSettlementItemListParams>,
) -> Result<SettlementPageView<SettlementItemDisplayView>> {
    let page = crate::wiring::scoped_settlement_service(
        state.application().db(),
        state.application().authorization(),
    )
    .supplier_settlement_item_list(&params, &actor)
    .await?;

    let page =
        SupplierSettlementReadService::new(state.application().db(), state.application().authorization())
            .item_page_view(page)
            .await?;
    Ok(ApiResponse::ok_with_data(page))
}

/// 查询供应商结算差异列表。
///
/// # 参数
/// * `state` - 应用状态
/// * `query` - 分页与筛选参数（`statement_item_id`/`status` 扁平传递）
///
/// # 返回
/// 返回契约形状的分页视图。
pub async fn supplier_settlement_difference_list(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Query(params): Query<SupplierSettlementDifferenceListParams>,
) -> Result<SettlementPageView<SupplierSettlementDifferenceView<SettlementEvidenceDisplayView>>> {
    let page = crate::wiring::scoped_settlement_service(
        state.application().db(),
        state.application().authorization(),
    )
    .supplier_settlement_difference_list(&params, &actor)
    .await?;

    let page =
        SupplierSettlementReadService::new(state.application().db(), state.application().authorization())
            .difference_page_view(page)
            .await?;
    Ok(ApiResponse::ok_with_data(page))
}

/// 登记结算差异处理结论（乐观锁；处理结果三元组成组校验）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 结算差异 ID
/// * `req` - 处理请求（含期望版本与结论状态）
///
/// # 返回
/// 返回处理后差异的响应视图。
pub async fn supplier_settlement_difference_decide(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<SettlementDifferenceDecisionRequest>,
) -> Result<SettlementDifferenceDecisionResult<SupplierSettlementDifferenceView<SettlementEvidenceDisplayView>>>
{
    let view = settlement_process(&state).decide_difference(&id, req, &actor).await?;

    let view =
        SupplierSettlementReadService::new(state.application().db(), state.application().authorization())
            .difference_result_view(view)
            .await?;
    Ok(ApiResponse::ok_with_data(view))
}

/// 追加不可变差异补证；不直接改变差异结论。
pub async fn supplier_settlement_difference_evidence_append(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<SettlementDifferenceEvidenceRequest>,
) -> Result<SettlementDifferenceEvidenceResult<SettlementEvidenceDisplayView>> {
    let view = settlement_process(&state).append_difference_evidence(&id, req, &actor).await?;
    let view =
        SupplierSettlementReadService::new(state.application().db(), state.application().authorization())
            .evidence_result_view(view)
            .await?;
    Ok(ApiResponse::ok_with_data(view))
}

/// 显式交接对账负责人；开放复核任务不改派。
pub async fn supplier_settlement_statement_handover(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<HandoverSettlementRequest>,
) -> Result<HandoverSettlementView> {
    let view = settlement_process(&state).handover_statement(&id, req, &actor).await?;
    Ok(ApiResponse::ok_with_data(view))
}

/// 返回可交接的合格有效人员。
pub async fn supplier_settlement_statement_handover_candidates(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> Result<Vec<HandoverCandidateView>> {
    let view = settlement_process(&state).handover_candidates(&id, &actor).await?;
    Ok(ApiResponse::ok_with_data(view))
}

/// 独立改派差异处理人，不得强制等于对账负责人。
pub async fn supplier_settlement_statement_reassign_difference_handler(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<ReassignSettlementDifferenceHandlerRequest>,
) -> Result<ReassignSettlementDifferenceHandlerView> {
    let view = settlement_process(&state).reassign_difference_handler(&id, req, &actor).await?;
    Ok(ApiResponse::ok_with_data(view))
}
