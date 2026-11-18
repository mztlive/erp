//! 域 D02 `document_registry` 的 HTTP handler。
//!
//! Handler 只做协议适配：`Validate`（DTO 内联）→ Service 调用 → `ApiResponse`，
//! 直接复用 `erp_workflow` 单据注册 DTO，禁止重复定义同构类型、禁止直连数据库。

use application_core::AuditActor;
use axum::Extension;
use axum::extract::{Path, State};
use erp_core::ids::BusinessDocumentId;
use erp_workflow::service::document_registry::{
    AppendWorkflowActionRequest, BusinessDocumentListParams, BusinessDocumentView,
    CreateDocumentParticipantRequest, CreateDocumentRelationRequest, DocumentParticipantView,
    DocumentRegistryService, DocumentRelationView, PageView, RegisterBusinessDocumentRequest,
    WorkflowActionListParams, WorkflowActionView,
};

use crate::http::errors::Result;
use crate::http::extractor::{Json, Query};
use crate::http::response::ApiResponse;
use crate::http::state::HttpState;
use crate::wiring::workflow::{workflow_audit, workflow_object_facts};

fn document_registry_service(state: &HttpState) -> DocumentRegistryService {
    DocumentRegistryService::with_ports(
        state.application().db(),
        state.application().authorization(),
        workflow_audit(state.application().db()),
        workflow_object_facts(state.application().db()),
    )
}

/// 按请求条件查询单据注册列表。
/// `query` 为分页与筛选参数（`document_type`/`document_no` 扁平传递）。
pub async fn business_document_list(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Query(params): Query<BusinessDocumentListParams>,
) -> Result<PageView<BusinessDocumentView>> {
    let page = document_registry_service(&state).business_document_list(&params, &actor).await?;

    Ok(ApiResponse::ok_with_data(page))
}

/// 注册业务单据（幂等：同身份同 ID 返回已存在行）。
/// `req` 为注册请求（`{ document_type, document_no }`）。
/// 返回注册行视图（幂等命中时返回已存在行）。
pub async fn business_document_register(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<RegisterBusinessDocumentRequest>,
) -> Result<BusinessDocumentView> {
    let view = document_registry_service(&state).register_business_document(req, &actor).await?;

    Ok(ApiResponse::ok_with_data(view))
}

/// 查询单据注册详情。
/// 返回注册行视图。
pub async fn business_document_detail(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> Result<BusinessDocumentView> {
    let view = document_registry_service(&state).business_document_detail(&id, &actor).await?;

    Ok(ApiResponse::ok_with_data(view))
}

/// 按请求条件查询工作流动作列表。
/// `query` 为分页与筛选参数（`document_id`/`actor_id`/`action_type`）。
pub async fn workflow_action_list(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Query(params): Query<WorkflowActionListParams>,
) -> Result<PageView<WorkflowActionView>> {
    let page = document_registry_service(&state).workflow_action_list(&params, &actor).await?;

    Ok(ApiResponse::ok_with_data(page))
}

/// 追加工作流动作（提交/通过/驳回/确认/作废/完成）。
/// `req` 为追加请求（`{ document_id, action_type, from_status, to_status }`）。
/// 返回新建的动作视图。
pub async fn workflow_action_create(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<AppendWorkflowActionRequest>,
) -> Result<WorkflowActionView> {
    // 责任角色由 Service 按操作人账号类型注入（HTTP 层不携带角色字段）。
    let view = document_registry_service(&state).append_workflow_action(req, &actor).await?;

    Ok(ApiResponse::ok_with_data(view))
}

/// 查询单据的全部关系（出向 + 入向）。
/// 返回关系视图列表。
pub async fn document_relation_list(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> Result<Vec<DocumentRelationView>> {
    let relations = document_registry_service(&state)
        .document_relation_list(&BusinessDocumentId::new(id), &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(relations))
}

/// 建立单据关系（变更/退货/退款/冲正/红票/派生）。
/// `req` 为创建请求（`{ from_document_id, to_document_id, relation_type }`）。
/// 返回新建的关系视图。
pub async fn document_relation_create(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<CreateDocumentRelationRequest>,
) -> Result<DocumentRelationView> {
    let view = document_registry_service(&state).create_document_relation(req, &actor).await?;

    Ok(ApiResponse::ok_with_data(view))
}

/// 按参与人查询其参与过的全部单据（“我的参与单据”）。
/// `query` 为查询参数（`user_id` 必填）。
/// 返回参与记录视图列表。
pub async fn document_participant_list(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Query(query): Query<DocumentParticipantListQuery>,
) -> Result<Vec<DocumentParticipantView>> {
    let items = document_registry_service(&state).document_participant_list(&query.user_id, &actor).await?;

    Ok(ApiResponse::ok_with_data(items))
}

/// 登记单据参与人（只追加不删除，客户历史查看权依据）。
/// 返回新建的参与人视图。
pub async fn document_participant_create(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<CreateDocumentParticipantRequest>,
) -> Result<DocumentParticipantView> {
    // 记录人由 Service 按操作人账号 ID 注入（HTTP 层不携带记录人字段）。
    let view = document_registry_service(&state).create_document_participant(req, &actor).await?;

    Ok(ApiResponse::ok_with_data(view))
}

/// 单据参与人列表查询参数（`user_id` 必填）。
///
/// HTTP 形态差异说明：Service 方法接收裸 `&str`，此处是 Query 提取所需的最薄
/// 包装（仅声明字段，无业务逻辑）。
#[derive(Debug, Clone, serde::Deserialize)]
pub struct DocumentParticipantListQuery {
    /// 参与人用户 ID。
    pub user_id: String,
}
