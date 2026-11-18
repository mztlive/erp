//! 域 D12 `contract` 的 HTTP handler。
//!
//! Handler 只做协议适配：`Validate`（DTO 内联）→ Service 调用 → `ApiResponse`，
//! 直接复用 `erp_contract` 的 DTO，禁止重复定义同构类型、禁止直连数据库。

pub mod imports;
mod template_upload;
pub mod templates;

use application_core::AuditActor;
use axum::Extension;
use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::header::{CACHE_CONTROL, CONTENT_TYPE, X_CONTENT_TYPE_OPTIONS};
use axum::http::{HeaderValue, StatusCode};
use axum::response::Response;
use erp_contract::{
    ArchiveContractRevisionRequest, ContractDetailView, ContractListParams, ContractListView,
    ContractSalesAccessRequest, ContractView, CreateContractRequest, TerminateContractRequest,
};
use erp_support::FileAssetView;
use tracing::error;

use super::file_asset::prepare_asset_response;
use crate::http::errors::{Error, Result};
use crate::http::extractor::{Json, Query};
use crate::http::response::ApiResponse;
use crate::http::state::HttpState;

/// 在维护合同共享范围时调整本合同的明确销售共享名单；此入口沿用客户人员分配资格。
pub async fn contract_sales_access(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(request): Json<ContractSalesAccessRequest>,
) -> Result<ContractDetailView> {
    let view = state.application().contract_service().set_sales_access(&id, request, &actor).await?;
    Ok(ApiResponse::ok_with_data(view))
}

/// 按请求条件查询合同列表。
/// 返回契约形状的分页视图及范围元数据。
pub async fn contract_list(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Query(params): Query<ContractListParams>,
) -> Result<ContractListView> {
    let page = state.application().contract_service().contract_list(&params, &actor).await?;

    Ok(ApiResponse::ok_with_data(page))
}

/// 保留旧的合同手填归档入口，但始终拒绝请求；首次归档必须通过合同 PDF 自动识别导入。
pub async fn contract_create(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<CreateContractRequest>,
) -> Result<ContractView> {
    let _ = (state, actor, req);
    Err(Error::BadRequest("合同必须通过 PDF 自动识别导入，不接受手填归档".into()))
}

/// 查询合同详情（合同 + 全部不可变版本时间线）。
/// 返回详情视图（版本按序号倒序）。
pub async fn contract_detail(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> Result<ContractDetailView> {
    let view = state.application().contract_service().contract_detail(&id, &actor).await?;

    Ok(ApiResponse::ok_with_data(view))
}

/// 独立重验合同读取资格后返回合同 PDF 元数据。
/// 返回附件元数据；不暴露未授权合同的存在性。
pub async fn contract_file(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path((id, file_id)): Path<(String, String)>,
) -> Result<FileAssetView> {
    let mut view = state.application().contract_attachments().metadata(&id, &file_id, &actor).await?;
    prepare_asset_response(&state, &mut view);
    Ok(ApiResponse::ok_with_data(view))
}

/// 独立重验合同读取资格后以内联内容返回合同 PDF。
/// 返回 PDF 字节流。
pub async fn contract_file_preview(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path((id, file_id)): Path<(String, String)>,
) -> std::result::Result<Response, Error> {
    let view = state.application().contract_attachments().preview(&id, &file_id, &actor).await?;
    if view.content_type.as_str() != "application/pdf" {
        return Err(Error::Unprocessable("当前文件类型不支持在线预览".to_string()));
    }
    let content =
        state.application().storage().read(&view.storage_object_key).await.map_err(|storage_error| {
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

/// 关闭手填合同版本入口；追加正式版本必须通过 PDF 自动识别导入，此接口不切换当前版本。
pub async fn contract_archive_revision(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<ArchiveContractRevisionRequest>,
) -> Result<ContractDetailView> {
    let _ = (state, actor, id, req);
    Err(Error::BadRequest("合同新版本必须通过 PDF 自动识别导入".into()))
}

/// 终止合同（乐观锁；历史销售引用保持不变）。
/// `req` 为终止请求（含期望版本）。
/// 返回终止后的合同详情视图。
pub async fn contract_terminate(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<TerminateContractRequest>,
) -> Result<ContractDetailView> {
    let view = state.application().contract_service().terminate_contract(&id, req, &actor).await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::extract::FromRequest;
    use axum::http::{Request, StatusCode, Uri};
    use serde_json::json;

    use super::*;
    use crate::http::extractor::Query;

    // 合同 Query 解码必须消费范围版本与组织筛选，并拒绝旧姓名参数。
    //
    // 未知或已废弃姓名参数不得被忽略后返回全部合同。
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

    #[tokio::test]
    async fn contract_sales_access_body_accepts_shares_and_rejects_owner_forgery() {
        for (forged_owner, expected) in [(false, StatusCode::OK), (true, StatusCode::UNPROCESSABLE_ENTITY)] {
            let mut body = json!({
                "version": 1,
                "shared_sales_user_ids": ["sales-b"],
                "change_reason": "共享年度框架合同"
            });
            if forged_owner {
                body["sales_owner_user_id"] = json!("sales-b");
            }
            let request = Request::builder()
                .method("PUT")
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap();
            match Json::<ContractSalesAccessRequest>::from_request(request, &()).await {
                Ok(Json(input)) => {
                    assert_eq!(expected, StatusCode::OK);
                    assert_eq!(input.shared_sales_user_ids, ["sales-b"]);
                },
                Err(error) => assert_eq!(error.status(), expected),
            }
        }
    }
}

mod import_execution;
