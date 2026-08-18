//! 审批流程定义管理 HTTP 适配层。
//!
//! 只依赖 `ApprovalDefinitionService` 与定义 DTO；不得查询数据库或解释 ProcessKind。

mod http;

use axum::{
    extract::{Path, Query, State},
    http::HeaderMap,
    Extension, Json,
};
use entities::document_registry::DocumentType;
use services::{
    approval::definition::{definition_management_visibility, ApprovalDefinitionService},
    approval::definition_dto::{
        CreateDefinitionDraftRequest, DefinitionCatalogItem, DefinitionDetailView, DefinitionVersionItem,
        PublishDefinitionRequest, ReplaceDefinitionNodesRequest, RetireDefinitionRequest,
    },
    audit::AuditActor,
};

use crate::{
    app_state::AppState,
    core::{
        handler::approval_instance::error::{parse_version, ApprovalHttpError},
        response::ApiResponse,
    },
};

use self::http::{DefinitionLockHttpRequest, EligibleAssigneesQuery, ReplaceNodesHttpRequest};

/// 定义 Handler 结果。
type ApprovalResult<T> = std::result::Result<ApiResponse<T>, ApprovalHttpError>;

#[permission_macros::permission(
    group = "审批流程",
    group_desc = "固定单据类型审批流程定义管理",
    desc = "读取固定单据类型审批目录",
    resource = "approval_process",
    action = "read"
)]
/// 返回固定 20 行非敏感目录。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已认证操作人
/// * `headers` - 用于关联 ID
///
/// # 返回
/// 返回目录行；政策缺失时映射为 500。
///
/// # 错误
/// 政策或仓储失败时返回审批 HTTP 错误。
pub async fn definition_catalog(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    headers: HeaderMap,
) -> ApprovalResult<Vec<DefinitionCatalogItem>> {
    let visibility = definition_management_visibility(state.rbac().as_ref(), &actor)
        .await
        .map_err(|error| ApprovalHttpError::from_service(error, &headers))?;
    let items = definition_service(&state)
        .definition_catalog(&actor, &visibility)
        .await
        .map_err(|error| ApprovalHttpError::from_service(error, &headers))?;
    Ok(ApiResponse::ok_with_data(items))
}

#[permission_macros::permission(
    group = "审批流程",
    group_desc = "固定单据类型审批流程定义管理",
    desc = "读取某单据类型的定义版本",
    resource = "approval_process",
    action = "read"
)]
/// 列出某单据类型的定义版本。
///
/// # 错误
/// 无读取权或不适用类型时不泄露存在性。
pub async fn definition_versions(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    headers: HeaderMap,
    Path(document_type): Path<DocumentType>,
) -> ApprovalResult<Vec<DefinitionVersionItem>> {
    let visibility = definition_management_visibility(state.rbac().as_ref(), &actor)
        .await
        .map_err(|error| ApprovalHttpError::from_service(error, &headers))?;
    let versions = definition_service(&state)
        .definition_versions(document_type, &actor, &visibility)
        .await
        .map_err(|error| ApprovalHttpError::from_service(error, &headers))?;
    Ok(ApiResponse::ok_with_data(versions))
}

#[permission_macros::permission(
    group = "审批流程",
    group_desc = "固定单据类型审批流程定义管理",
    desc = "读取审批流程定义详情",
    resource = "approval_process",
    action = "read"
)]
/// 返回定义图详情。
///
/// # 错误
/// 无权或不存在时返回不泄露存在性的错误。
pub async fn definition_detail(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> ApprovalResult<DefinitionDetailView> {
    let visibility = definition_management_visibility(state.rbac().as_ref(), &actor)
        .await
        .map_err(|error| ApprovalHttpError::from_service(error, &headers))?;
    let view = definition_service(&state)
        .definition_detail(&id, &actor, &visibility)
        .await
        .map_err(|error| ApprovalHttpError::from_service(error, &headers))?;
    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "审批流程",
    group_desc = "固定单据类型审批流程定义管理",
    desc = "创建更高版本审批流程草稿",
    resource = "approval_process",
    action = "create"
)]
/// 创建定义草稿。
///
/// actor 只从认证上下文注入，请求体不得携带源定义 ID。
///
/// # 错误
/// 无发布源、已有活动草稿或权限不足时返回稳定错误。
pub async fn create_definition_draft(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    headers: HeaderMap,
    Json(request): Json<CreateDefinitionDraftRequest>,
) -> ApprovalResult<DefinitionDetailView> {
    let view = definition_service(&state)
        .create_definition_draft(request, &actor)
        .await
        .map_err(|error| ApprovalHttpError::from_service(error, &headers))?;
    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "审批流程",
    group_desc = "固定单据类型审批流程定义管理",
    desc = "整组替换草稿节点",
    resource = "approval_process",
    action = "edit"
)]
/// 整组替换草稿节点。
///
/// 新增节点不接受 `node_key`；已有节点只接受本定义内 `node_id`。
///
/// # 错误
/// 锁冲突或节点不合法时返回稳定错误。
pub async fn replace_definition_nodes(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(request): Json<ReplaceNodesHttpRequest>,
) -> ApprovalResult<DefinitionDetailView> {
    let command = replace_nodes_command(id, request, &headers)?;
    let view = definition_service(&state)
        .replace_definition_nodes(command, &actor)
        .await
        .map_err(|error| ApprovalHttpError::from_service(error, &headers))?;
    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "审批流程",
    group_desc = "固定单据类型审批流程定义管理",
    desc = "发布审批流程草稿",
    resource = "approval_process",
    action = "publish"
)]
/// 发布草稿并退役旧版本。
///
/// # 错误
/// 图、人员或锁冲突时返回稳定错误。
pub async fn publish_definition(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(request): Json<DefinitionLockHttpRequest>,
) -> ApprovalResult<DefinitionDetailView> {
    let command = publish_command(id, request, &headers)?;
    let view = definition_service(&state)
        .publish_definition(command, &actor)
        .await
        .map_err(|error| ApprovalHttpError::from_service(error, &headers))?;
    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "审批流程",
    group_desc = "固定单据类型审批流程定义管理",
    desc = "退役当前已发布审批流程",
    resource = "approval_process",
    action = "retire"
)]
/// 退役当前已发布定义。
///
/// # 错误
/// 目标不是当前发布版本或锁冲突时返回稳定错误。
pub async fn retire_definition(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(request): Json<DefinitionLockHttpRequest>,
) -> ApprovalResult<DefinitionDetailView> {
    let command = retire_command(id, request, &headers)?;
    let view = definition_service(&state)
        .retire_definition(command, &actor)
        .await
        .map_err(|error| ApprovalHttpError::from_service(error, &headers))?;
    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "审批流程",
    group_desc = "固定单据类型审批流程定义管理",
    desc = "按定义期规则搜索可选审批人",
    resource = "approval_process",
    action = "read"
)]
/// 服务端过滤定义期可选审批人。
///
/// 只执行定义期静态过滤，不得复用运行期改派候选。
///
/// # 错误
/// 查询非法时返回 422；应用端口未接入时失败关闭。
pub async fn eligible_assignees(
    headers: HeaderMap,
    Path(_document_type): Path<DocumentType>,
    Query(query): Query<EligibleAssigneesQuery>,
) -> ApprovalResult<Vec<serde_json::Value>> {
    query
        .normalized_limit()
        .map_err(|message| ApprovalHttpError::unprocessable(message, &headers))?;
    Err(ApprovalHttpError::from(services::Error::Internal(
        "定义期审批人候选应用端口尚未接入，已按安全策略拒绝".to_string(),
    )))
}

/// 构造定义管理服务。
fn definition_service(state: &AppState) -> ApprovalDefinitionService {
    ApprovalDefinitionService::new(state.db(), state.rbac())
}

/// 把 HTTP 节点写请求转为服务命令。
///
/// # 错误
/// 锁版本非法时返回 400。
fn replace_nodes_command(
    definition_id: String,
    request: ReplaceNodesHttpRequest,
    headers: &HeaderMap,
) -> Result<ReplaceDefinitionNodesRequest, ApprovalHttpError> {
    Ok(ReplaceDefinitionNodesRequest {
        definition_id,
        expected_definition_lock_version: parse_version(
            &request.expected_definition_lock_version,
            "定义锁",
            headers,
        )?,
        nodes: request.nodes,
    })
}

/// 把 HTTP 发布请求转为服务命令。
///
/// # 错误
/// 锁版本非法时返回 400。
fn publish_command(
    definition_id: String,
    request: DefinitionLockHttpRequest,
    headers: &HeaderMap,
) -> Result<PublishDefinitionRequest, ApprovalHttpError> {
    Ok(PublishDefinitionRequest {
        definition_id,
        expected_definition_lock_version: parse_version(
            &request.expected_definition_lock_version,
            "定义锁",
            headers,
        )?,
        idempotency_key: request.idempotency_key,
    })
}

/// 把 HTTP 退役请求转为服务命令。
///
/// # 错误
/// 锁版本非法时返回 400。
fn retire_command(
    definition_id: String,
    request: DefinitionLockHttpRequest,
    headers: &HeaderMap,
) -> Result<RetireDefinitionRequest, ApprovalHttpError> {
    Ok(RetireDefinitionRequest {
        definition_id,
        expected_definition_lock_version: parse_version(
            &request.expected_definition_lock_version,
            "定义锁",
            headers,
        )?,
        idempotency_key: request.idempotency_key,
    })
}

#[cfg(test)]
mod tests {
    use axum::http::HeaderMap;
    use serde_json::json;

    use super::http::{DefinitionLockHttpRequest, ReplaceNodesHttpRequest};
    use super::{publish_command, replace_nodes_command};

    #[test]
    fn replace_nodes_injects_path_id_and_parses_string_version() {
        let command = replace_nodes_command(
            "def-1".to_string(),
            ReplaceNodesHttpRequest {
                expected_definition_lock_version: "3".to_string(),
                nodes: Vec::new(),
            },
            &HeaderMap::new(),
        )
        .expect("合法版本");
        assert_eq!(command.definition_id, "def-1");
        assert_eq!(command.expected_definition_lock_version, 3);
    }

    #[test]
    fn lock_command_rejects_zero_version() {
        let error = publish_command(
            "def-1".to_string(),
            DefinitionLockHttpRequest {
                expected_definition_lock_version: "0".to_string(),
                idempotency_key: "k1".to_string(),
            },
            &HeaderMap::new(),
        )
        .expect_err("零版本必须拒绝");
        assert_eq!(error.status(), axum::http::StatusCode::BAD_REQUEST);
    }

    #[test]
    fn publish_request_rejects_actor_and_definition_fields() {
        assert!(serde_json::from_value::<DefinitionLockHttpRequest>(json!({
            "expected_definition_lock_version": "1",
            "idempotency_key": "k1",
            "next_node": "n2"
        }))
        .is_err());
    }
}
