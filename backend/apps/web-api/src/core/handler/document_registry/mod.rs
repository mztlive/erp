//! 域 D02 `document_registry` 的 HTTP handler。
//!
//! Handler 只做协议适配：`Validate`（DTO 内联）→ Service 调用 → `ApiResponse`，
//! 直接复用 `services::document_registry` 的 DTO，禁止重复定义同构类型、禁止直连数据库。

use axum::{
    extract::{Path, Query, State},
    Extension, Json,
};
use services::{
    audit::AuditActor,
    document_registry::{
        AppendWorkflowActionRequest, BusinessDocumentListParams, BusinessDocumentView,
        CreateDocumentParticipantRequest, CreateDocumentRelationRequest, DocumentParticipantView,
        DocumentRegistryService, DocumentRelationView, PageView, RegisterBusinessDocumentRequest,
        WorkflowActionListParams, WorkflowActionView,
    },
};

use crate::{
    app_state::AppState,
    core::{errors::Result, response::ApiResponse},
};

#[permission_macros::permission(
    group = "单据注册",
    group_desc = "跨域单据稳定注册表与工作流动作",
    desc = "查询单据注册列表",
    resource = "business_document",
    action = "list"
)]
/// 查询单据注册列表。
///
/// # 参数
/// * `state` - 应用状态
/// * `query` - 分页与筛选参数（`document_type`/`document_no` 扁平传递）
///
/// # 返回
/// 返回契约形状的分页视图（`items`/`total`/`page`/`page_size`）。
pub async fn business_document_list(
    State(state): State<AppState>,
    Query(params): Query<BusinessDocumentListParams>,
) -> Result<PageView<BusinessDocumentView>> {
    let page = DocumentRegistryService::new(state.db())
        .business_document_list(&params)
        .await?;

    Ok(ApiResponse::ok_with_data(page))
}

#[permission_macros::permission(
    group = "单据注册",
    group_desc = "跨域单据稳定注册表与工作流动作",
    desc = "注册业务单据（幂等）",
    resource = "business_document",
    action = "create"
)]
/// 注册业务单据（幂等：同身份同 ID 返回已存在行）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `req` - 注册请求（`{ document_type, document_no }`）
///
/// # 返回
/// 返回注册行视图（幂等命中时返回已存在行）。
pub async fn business_document_register(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<RegisterBusinessDocumentRequest>,
) -> Result<BusinessDocumentView> {
    let view = DocumentRegistryService::new(state.db())
        .register_business_document(req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "单据注册",
    group_desc = "跨域单据稳定注册表与工作流动作",
    desc = "查询单据注册详情",
    resource = "business_document",
    action = "detail"
)]
/// 查询单据注册详情。
///
/// # 参数
/// * `state` - 应用状态
/// * `id` - 单据注册 ID
///
/// # 返回
/// 返回注册行视图。
pub async fn business_document_detail(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<BusinessDocumentView> {
    let view = DocumentRegistryService::new(state.db())
        .business_document_detail(&id)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "单据注册",
    group_desc = "跨域单据稳定注册表与工作流动作",
    desc = "查询工作流动作列表",
    resource = "workflow_action",
    action = "list"
)]
/// 查询工作流动作列表。
///
/// # 参数
/// * `state` - 应用状态
/// * `query` - 分页与筛选参数（`document_id`/`actor_id`/`action_type`）
///
/// # 返回
/// 返回契约形状的分页视图（`items`/`total`/`page`/`page_size`）。
pub async fn workflow_action_list(
    State(state): State<AppState>,
    Query(params): Query<WorkflowActionListParams>,
) -> Result<PageView<WorkflowActionView>> {
    let page = DocumentRegistryService::new(state.db())
        .workflow_action_list(&params)
        .await?;

    Ok(ApiResponse::ok_with_data(page))
}

#[permission_macros::permission(
    group = "单据注册",
    group_desc = "跨域单据稳定注册表与工作流动作",
    desc = "追加工作流动作",
    resource = "workflow_action",
    action = "create"
)]
/// 追加工作流动作（提交/通过/驳回/确认/作废/完成）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `req` - 追加请求（`{ document_id, action_type, from_status, to_status }`）
///
/// # 返回
/// 返回新建的动作视图。
pub async fn workflow_action_create(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<AppendWorkflowActionRequest>,
) -> Result<WorkflowActionView> {
    // 责任角色由 Service 按操作人账号类型注入（HTTP 层不携带角色字段）。
    let view = DocumentRegistryService::new(state.db())
        .append_workflow_action(req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "单据注册",
    group_desc = "跨域单据稳定注册表与工作流动作",
    desc = "查询单据关系列表",
    resource = "document_relation",
    action = "list"
)]
/// 查询单据的全部关系（出向 + 入向）。
///
/// # 参数
/// * `state` - 应用状态
/// * `id` - 业务单据 ID
///
/// # 返回
/// 返回关系视图列表。
pub async fn document_relation_list(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Vec<DocumentRelationView>> {
    let relations = DocumentRegistryService::new(state.db())
        .document_relation_list(&entities::ids::BusinessDocumentId::new(id))
        .await?;

    Ok(ApiResponse::ok_with_data(relations))
}

#[permission_macros::permission(
    group = "单据注册",
    group_desc = "跨域单据稳定注册表与工作流动作",
    desc = "建立单据关系",
    resource = "document_relation",
    action = "create"
)]
/// 建立单据关系（变更/退货/退款/冲正/红票/派生）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `req` - 创建请求（`{ from_document_id, to_document_id, relation_type }`）
///
/// # 返回
/// 返回新建的关系视图。
pub async fn document_relation_create(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<CreateDocumentRelationRequest>,
) -> Result<DocumentRelationView> {
    let view = DocumentRegistryService::new(state.db())
        .create_document_relation(req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "单据注册",
    group_desc = "跨域单据稳定注册表与工作流动作",
    desc = "查询单据参与人列表",
    resource = "document_participant",
    action = "list"
)]
/// 按参与人查询其参与过的全部单据（“我的参与单据”）。
///
/// # 参数
/// * `state` - 应用状态
/// * `query` - 查询参数（`user_id` 必填）
///
/// # 返回
/// 返回参与记录视图列表。
pub async fn document_participant_list(
    State(state): State<AppState>,
    Query(query): Query<DocumentParticipantListQuery>,
) -> Result<Vec<DocumentParticipantView>> {
    let items = DocumentRegistryService::new(state.db())
        .document_participant_list(&query.user_id)
        .await?;

    Ok(ApiResponse::ok_with_data(items))
}

#[permission_macros::permission(
    group = "单据注册",
    group_desc = "跨域单据稳定注册表与工作流动作",
    desc = "登记单据参与人",
    resource = "document_participant",
    action = "create"
)]
/// 登记单据参与人（只追加不删除，客户历史查看权依据）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `req` - 创建请求（`{ document_id, participant_role, participant_user_id,
///   participant_name }`）
///
/// # 返回
/// 返回新建的参与人视图。
pub async fn document_participant_create(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<CreateDocumentParticipantRequest>,
) -> Result<DocumentParticipantView> {
    // 记录人由 Service 按操作人账号 ID 注入（HTTP 层不携带记录人字段）。
    let view = DocumentRegistryService::new(state.db())
        .create_document_participant(req, &actor)
        .await?;

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
