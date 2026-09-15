//! 域 D12 `contract` 的 HTTP handler。
//!
//! Handler 只做协议适配：`Validate`（DTO 内联）→ Service 调用 → `ApiResponse`，
//! 直接复用 `erp_contract` 的 DTO，禁止重复定义同构类型、禁止直连数据库。

use application_core::AuditActor;
use axum::body::Body;
use axum::extract::{Multipart, Path, Query, State};
use axum::http::header::{CACHE_CONTROL, CONTENT_TYPE, X_CONTENT_TYPE_OPTIONS};
use axum::http::{HeaderValue, StatusCode};
use axum::response::Response;
use axum::{Extension, Json};
use erp_contract::{
    ArchiveContractRevisionRequest, ContractDetailView, ContractListParams, ContractListView, ContractView,
    CreateContractRequest, TerminateContractRequest, UploadContractRequest, UploadContractView,
};
use erp_support::{FileAssetView, RetentionClass, SensitivityClass};
use tracing::error;

use super::file_asset::{
    extract_asset_file_with_limit, prepare_asset_response, should_compensate_pending_assets, store_asset_file,
};
use crate::app_state::AppState;
use crate::core::errors::{Error, Result};
use crate::core::handler::customer::ensure_customer_access;
use crate::core::response::ApiResponse;
use crate::core::upload;

#[permission_macros::permission(
    group = "合同",
    group_desc = "合同 PDF 档案管理",
    desc = "查询合同列表",
    resource = "contract",
    action = "list"
)]
/// 查询合同列表。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已认证操作人
/// * `query` - 分页与筛选参数（扁平传递）
///
/// # 返回
/// 返回契约形状的分页视图及范围元数据。
pub async fn contract_list(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Query(params): Query<ContractListParams>,
) -> Result<ContractListView> {
    let page = state.contract_service().contract_list(&params, &actor).await?;

    Ok(ApiResponse::ok_with_data(page))
}

#[permission_macros::permission(
    group = "合同",
    group_desc = "合同 PDF 档案管理",
    desc = "首次归档合同 PDF",
    resource = "contract",
    action = "create"
)]
/// 首次归档合同（合同身份 + 首个不可变版本 + PDF 关联原子形成）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `req` - 创建请求（`contract_no`、客户、结算主体与版本快照）
///
/// # 返回
/// 返回新建合同的响应视图。
pub async fn contract_create(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<CreateContractRequest>,
) -> Result<ContractView> {
    ensure_customer_access(&state, &actor, "detail", &req.customer_id).await?;
    let view = state.contract_service().create_contract(req, &actor).await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "合同",
    group_desc = "合同 PDF 档案管理",
    desc = "一次上传并归档合同 PDF",
    resource = "contract",
    action = "create"
)]
/// 一次接收合同 PDF 与业务字段，并原子登记文件元数据、合同及首修订。
///
/// 对象存储不具备 MongoDB 事务能力；数据库事务失败时本入口立即删除刚上传的
/// 对象作为补偿，避免留下未登记文件。
pub async fn contract_upload(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    mut multipart: Multipart,
) -> Result<UploadContractView> {
    let file = extract_asset_file_with_limit(&mut multipart, upload::MAX_CONTRACT_PDF_BYTES).await?;
    let mut command = None;
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|_| crate::core::errors::Error::BadRequest("Multipart 表单无效".to_string()))?
    {
        if field.name() != Some("command") {
            continue;
        }
        let text = field
            .text()
            .await
            .map_err(|_| crate::core::errors::Error::BadRequest("合同命令读取失败".to_string()))?;
        command = Some(
            serde_json::from_str::<UploadContractRequest>(&text)
                .map_err(|_| crate::core::errors::Error::BadRequest("合同命令格式无效".to_string()))?,
        );
        break;
    }
    let command =
        command.ok_or_else(|| crate::core::errors::Error::BadRequest("缺少合同命令".to_string()))?;
    ensure_customer_access(&state, &actor, "detail", command.customer_id.as_ref()).await?;
    let asset_request =
        store_asset_file(&state, file, SensitivityClass::Sensitive, RetentionClass::LongTerm, None).await?;
    let object_key = asset_request.storage_object_key.clone();
    let result =
        erp_processes::upload_contract(state.db(), state.rbac(), command, asset_request, actor).await;
    let view = match result {
        Ok(view) => view,
        Err(error) => {
            if should_compensate_pending_assets(&error) {
                let _ = state.storage().delete(&object_key).await;
            }
            return Err(error.into());
        },
    };
    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "合同",
    group_desc = "合同 PDF 档案管理",
    desc = "查询合同详情",
    resource = "contract",
    action = "detail"
)]
/// 查询合同详情（合同 + 全部不可变版本时间线）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已认证操作人
/// * `id` - 合同 ID
///
/// # 返回
/// 返回详情视图（版本按序号倒序）。
pub async fn contract_detail(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> Result<ContractDetailView> {
    let view = state.contract_service().contract_detail(&id, &actor).await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "合同",
    group_desc = "合同 PDF 档案管理",
    desc = "查询合同附件",
    resource = "contract",
    action = "detail"
)]
/// 独立重验合同读取资格后返回合同 PDF 元数据。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已认证操作人
/// * `id` - 合同 ID
/// * `file_id` - 文件资产 ID
///
/// # 返回
/// 返回附件元数据；不暴露未授权合同的存在性。
pub async fn contract_file(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path((id, file_id)): Path<(String, String)>,
) -> Result<FileAssetView> {
    state.contract_service().require_attachment(&actor, &id, &file_id).await?;
    let mut view = state.file_asset_service().file_asset_detail(&file_id).await?;
    prepare_asset_response(&state, &mut view);
    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "合同",
    group_desc = "合同 PDF 档案管理",
    desc = "预览合同附件",
    resource = "contract",
    action = "detail"
)]
/// 独立重验合同读取资格后以内联内容返回合同 PDF。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已认证操作人
/// * `id` - 合同 ID
/// * `file_id` - 文件资产 ID
///
/// # 返回
/// 返回 PDF 字节流。
pub async fn contract_file_preview(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path((id, file_id)): Path<(String, String)>,
) -> std::result::Result<Response, Error> {
    state.contract_service().require_attachment(&actor, &id, &file_id).await?;
    let view = state.file_asset_service().file_asset_preview(&file_id, &actor).await?;
    if view.content_type.as_str() != "application/pdf" {
        return Err(Error::Unprocessable("当前文件类型不支持在线预览".to_string()));
    }
    let content = state.storage().read(&view.storage_object_key).await.map_err(|storage_error| {
        error!(error = %storage_error, file_asset_id = %file_id, "Failed to read contract PDF");
        Error::Internal("Object storage operation failed".to_string())
    })?;
    let content_type = HeaderValue::from_str(&view.content_type)
        .unwrap_or_else(|_| HeaderValue::from_static("application/pdf"));
    let mut response = Response::new(Body::from(content));
    *response.status_mut() = StatusCode::OK;
    response.headers_mut().insert(CONTENT_TYPE, content_type);
    response.headers_mut().insert(CACHE_CONTROL, HeaderValue::from_static("private, no-store"));
    response.headers_mut().insert(X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
    Ok(response)
}

#[permission_macros::permission(
    group = "合同",
    group_desc = "合同 PDF 档案管理",
    desc = "归档合同新版本",
    resource = "contract",
    action = "update"
)]
/// 归档合同新版本（追加不可变版本并切换当前版本指针，乐观锁）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 合同 ID
/// * `req` - 追加版本请求（含期望版本）
///
/// # 返回
/// 返回追加后的合同详情视图。
pub async fn contract_archive_revision(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<ArchiveContractRevisionRequest>,
) -> Result<ContractDetailView> {
    let view = state.contract_service().archive_contract_revision(&id, req, &actor).await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "合同",
    group_desc = "合同 PDF 档案管理",
    desc = "终止合同",
    resource = "contract",
    action = "update"
)]
/// 终止合同（乐观锁；历史销售引用保持不变）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 合同 ID
/// * `req` - 终止请求（含期望版本）
///
/// # 返回
/// 返回终止后的合同详情视图。
pub async fn contract_terminate(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<TerminateContractRequest>,
) -> Result<ContractDetailView> {
    let view = state.contract_service().terminate_contract(&id, req, &actor).await?;

    Ok(ApiResponse::ok_with_data(view))
}

/// 按资源动作重验合同对象范围；缺动作拒绝，缺范围不得补公司。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已认证操作人
/// * `action` - 已注册的合同动作
/// * `contract_id` - 目标合同
///
/// # 返回
/// 对象在范围内时成功。
///
/// # 错误
/// 读取动作对不可见对象返回 NotFound；写动作返回 Forbidden。
///
/// # 关键业务约束
/// 销售建单所选合同必须独立走合同 v2，不得用客户范围代替。
pub(crate) async fn ensure_contract_access(
    state: &AppState,
    actor: &AuditActor,
    action: &str,
    contract_id: &str,
) -> std::result::Result<(), Error> {
    erp_processes::adapters::contract_access(state.db(), state.rbac())
        .require(actor, action, contract_id)
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use axum::extract::Query;
    use axum::http::Uri;

    use super::*;

    /// 合同 Query 解码必须消费范围版本与组织筛选，并拒绝旧姓名参数。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 无。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// 未知或已废弃姓名参数不得被忽略后返回全部合同。
    #[test]
    fn contract_scope_version_and_org_filters_decode_from_url() {
        let uri: Uri =
            "/?page=2&page_size=25&scope_version=v1&owner_user_ids=a,b&org_unit_ids=org-1&include_descendants=true"
                .parse()
                .unwrap();
        let Query(params) = Query::<ContractListParams>::try_from_uri(&uri).unwrap();
        assert_eq!(params.page, Some(2));
        assert_eq!(params.scope_version.as_deref(), Some("v1"));
        assert!(params.owner_user_ids.is_some());
        assert_eq!(params.org_unit_ids.unwrap().as_slice(), &["org-1".to_string()]);
        assert_eq!(params.include_descendants, Some(true));
        let legacy: Uri = "/?owner=张三".parse().unwrap();
        assert!(Query::<ContractListParams>::try_from_uri(&legacy).is_err());
    }
}
