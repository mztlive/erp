//! 识别导入 HTTP 协议；识别与用户确认分别接收控制命令和业务字段。
use application_core::AuditActor;
use axum::body::Body;
use axum::extract::{Multipart, Path, Query, State};
use axum::http::{HeaderValue, header};
use axum::response::Response;
use axum::{Extension, Json};
use erp_contract::PageView;
use erp_contract::entity::recognition::{ConfirmImport, ImportCommand, ImportView};
use erp_processes::contract_import::inspect_pdf;
use erp_support::{RetentionClass, SensitivityClass};
use serde::Deserialize;

use crate::app_state::AppState;
use crate::core::errors::{Error, Result};
use crate::core::handler::file_asset::{
    AssetFile, should_compensate_pending_assets, store_asset_file, validate_asset_file,
};
use crate::core::response::ApiResponse;
use crate::core::{contract_import_worker, upload};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImportListQuery {
    pub page: Option<u64>,
    pub revision_contract_id: Option<String>,
}

#[permission_macros::permission(
    group = "合同",
    group_desc = "合同 PDF 档案管理",
    desc = "上传识别合同",
    resource = "contract",
    action = "create"
)]
/// 上传只登记导入任务，后续识别预填并由用户确认业务字段。
/// # 参数
/// * `state` / `actor` / `multipart` - 应用、认证人及文件与控制命令。
/// # 返回
/// 持久导入任务。
/// # 错误
/// 文件、未知字段、请求键或权限非法；事务未知时保留对象。
pub async fn upload(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    mut multipart: Multipart,
) -> Result<ImportView> {
    let file = read_file(&mut multipart).await?;
    if file.content_type != "application/pdf" {
        return Err(Error::BadRequest("合同仅支持 PDF".into()));
    }
    let command = parse_command(&mut multipart).await?;
    command.validate()?;
    let digest = inspect_pdf(file.content.clone()).await?;
    let asset =
        store_asset_file(&state, file, SensitivityClass::Sensitive, RetentionClass::LongTerm, None).await?;
    let object_key = asset.storage_object_key.clone();
    let result = state.contract_import_process().create(command, asset, digest, &actor).await;
    match result {
        Ok((view, used)) => {
            if !used {
                let _ = state.storage().delete(&object_key).await;
            }
            Ok(ApiResponse::ok_with_data(view))
        },
        Err(error) => {
            if should_compensate_pending_assets(&error) {
                let _ = state.storage().delete(&object_key).await;
            }
            Err(error.into())
        },
    }
}

async fn parse_command(multipart: &mut Multipart) -> std::result::Result<ImportCommand, Error> {
    let mut command = None;
    while let Some(field) =
        multipart.next_field().await.map_err(|_| Error::BadRequest("上传表单无效".into()))?
    {
        if field.name() != Some("command") || command.is_some() {
            return Err(Error::BadRequest("导入只接受一份文件和一个控制命令".into()));
        }
        let bytes = field.bytes().await.map_err(|_| Error::BadRequest("导入命令读取失败".into()))?;
        if bytes.len() > 4096 {
            return Err(Error::BadRequest("导入命令过长".into()));
        }
        command = Some(decode_command(&bytes)?);
    }
    command.ok_or_else(|| Error::BadRequest("缺少导入命令".into()))
}

fn decode_command(bytes: &[u8]) -> std::result::Result<ImportCommand, Error> {
    serde_json::from_slice(bytes)
        .map_err(|_| Error::BadRequest("导入命令无效，不允许手填合同业务字段".into()))
}

#[permission_macros::permission(
    group = "合同",
    group_desc = "合同 PDF 档案管理",
    desc = "查询本人合同导入",
    resource = "contract",
    action = "create"
)]
/// 分页查看本人的导入任务。
/// # 参数
/// * `state` / `actor` / `query` - 应用、认证人与页码。
/// # 返回
/// 本人任务页。
/// # 错误
/// 查询失败。
pub async fn list(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Query(query): Query<ImportListQuery>,
) -> Result<PageView<ImportView>> {
    Ok(ApiResponse::ok_with_data(
        state
            .contract_import_process()
            .list(query.page.unwrap_or(1), query.revision_contract_id.as_deref(), &actor)
            .await?,
    ))
}

#[permission_macros::permission(
    group = "合同",
    group_desc = "合同 PDF 档案管理",
    desc = "查询本人合同导入结果",
    resource = "contract",
    action = "create"
)]
/// 读取本人导入结果。
/// # 参数
/// * `state` / `actor` / `id` - 应用、认证人与任务。
/// # 返回
/// 可编辑草稿、原文依据与失败原因。
/// # 错误
/// 越权或查询失败。
pub async fn detail(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> Result<ImportView> {
    Ok(ApiResponse::ok_with_data(state.contract_import_process().detail(&id, &actor).await?))
}

#[permission_macros::permission(
    group = "合同",
    group_desc = "合同 PDF 档案管理",
    desc = "执行或重试合同识别",
    resource = "contract",
    action = "create"
)]
/// 执行或重试；请求不接受业务字段。
/// # 参数
/// * `state` / `actor` / `id` / `body` - 应用、认证人、任务与必须为空的正文。
/// # 返回
/// 已领取的任务状态；识别在独立后台任务中继续执行，归档由确认接口触发。
/// # 错误
/// 文件、并发、权限或数据库失败。
pub async fn run(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    body: axum::body::Bytes,
) -> Result<ImportView> {
    if !body.is_empty() {
        return Err(Error::BadRequest("识别请求不接受手填字段".into()));
    }
    Ok(ApiResponse::ok_with_data(contract_import_worker::start(state, actor, id).await?))
}

#[permission_macros::permission(
    group = "合同",
    group_desc = "合同 PDF 档案管理",
    desc = "确认合同识别信息并归档",
    resource = "contract",
    action = "create"
)]
/// 接收用户补充或修改的字段并归档。
/// # 参数
/// * `state` / `actor` / `id` / `command` - 应用、认证人、任务及确认命令。
/// # 返回
/// 已归档任务。
/// # 错误
/// 业务字段、主数据、权限、并发或事务错误。
pub async fn confirm(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(command): Json<ConfirmImport>,
) -> Result<ImportView> {
    Ok(ApiResponse::ok_with_data(state.contract_import_process().confirm(&id, command, &actor).await?))
}

#[permission_macros::permission(
    group = "合同",
    group_desc = "合同 PDF 档案管理",
    desc = "预览本人导入原文",
    resource = "contract",
    action = "create"
)]
/// 只通过本人任务提供原文件，禁止仅凭附件 ID 下载。
/// # 参数
/// * `state` / `actor` / `id` - 应用、认证人与任务。
/// # 返回
/// PDF 字节流。
/// # 错误
/// 无权、隔离或文件缺失。
pub async fn preview(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> std::result::Result<Response, Error> {
    let source = state.contract_import_process().source(&id, &actor).await?;
    let pdf = state
        .storage()
        .read(&source.storage_object_key)
        .await
        .map_err(|_| Error::Internal("合同文件读取失败".into()))?;
    let mut response = Response::new(Body::from(pdf));
    response.headers_mut().insert(header::CONTENT_TYPE, HeaderValue::from_static("application/pdf"));
    response.headers_mut().insert(header::CACHE_CONTROL, HeaderValue::from_static("private, no-store"));
    response.headers_mut().insert(header::X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
    Ok(response)
}

/// 识别接口拒绝文件前的任意业务字段，避免通用上传器跳过未知字段。
async fn read_file(multipart: &mut Multipart) -> std::result::Result<AssetFile, Error> {
    let mut field = multipart
        .next_field()
        .await
        .map_err(|_| Error::BadRequest("上传表单无效".into()))?
        .ok_or_else(|| Error::BadRequest("缺少合同 PDF".into()))?;
    if field.name() != Some("file") {
        return Err(Error::BadRequest("首个字段必须是合同文件，不接受手填信息".into()));
    }
    let file_name = field
        .file_name()
        .filter(|name| !name.trim().is_empty())
        .ok_or_else(|| Error::BadRequest("缺少文件名".into()))?
        .to_string();
    let content_type =
        field.content_type().ok_or_else(|| Error::BadRequest("缺少文件类型".into()))?.to_string();
    let mut content = Vec::new();
    while let Some(chunk) = field.chunk().await.map_err(|_| Error::BadRequest("合同读取失败".into()))? {
        if content.len().saturating_add(chunk.len()) > upload::MAX_CONTRACT_PDF_BYTES {
            return Err(Error::BadRequest("合同文件过大".into()));
        }
        content.extend_from_slice(&chunk);
    }
    validate_asset_file(AssetFile { file_name, content_type, content })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn confirmation_accepts_nullable_editable_fields_and_rejects_unknown_control_fields() {
        let command: ConfirmImport = serde_json::from_value(serde_json::json!({
            "version": 1, "fields": {"contract_no": "人工补充", "payment_terms": null}
        }))
        .unwrap();
        assert_eq!(command.fields.len(), 2);
        assert!(
            serde_json::from_value::<ConfirmImport>(serde_json::json!({
                "version": 1, "fields": {}, "owner_id": "other"
            }))
            .is_err()
        );
    }

    #[test]
    fn rejects_forged_business_fields_and_accepts_only_control_data() {
        assert!(decode_command(br#"{"request_key":"abcdefgh","contract_no":"FAKE"}"#).is_err());
        let command = decode_command(br#"{"request_key":"abcdefgh","expected_customer_id":"c1"}"#).unwrap();
        command.validate().unwrap();
    }
}
