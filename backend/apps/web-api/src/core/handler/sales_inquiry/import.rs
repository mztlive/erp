//! 客户 Excel 导入采用有界上传、预览与独立确认命令。
use application_core::AuditActor;
use axum::Extension;
use axum::extract::{Multipart, Path, State};
use erp_processes::sales_inquiry_import::{
    ConfirmInquiryImportCommand, InquiryImportPreviewView, SalesInquiryImportProcess,
};
use erp_sales::entity::sales_inquiry_import::{INQUIRY_IMPORT_MAX_BYTES, InquiryImportResult};

use crate::app_state::AppState;
use crate::core::errors::{Error, Result};
use crate::core::extractor::Json;
use crate::core::response::ApiResponse;

#[permission_macros::permission(
    group = "销售询价",
    group_desc = "询价需求与采购反馈协作",
    desc = "预览客户选品导入",
    resource = "sales_inquiry",
    action = "import"
)]
/// 解析客户选品工作簿并保存当前询价权限绑定的预览。
/// # 参数
/// `state` 为应用，`actor` 为当前销售，`id` 为询价；multipart 包含 file 和 expected_version。
/// # 返回
/// 返回逐工作表、逐行校验结果及可确认预览。
/// # 错误
/// 文件大小、类型、字段、询价权限或需求版本无效时拒绝。
pub async fn sales_import_preview(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    multipart: Multipart,
) -> Result<InquiryImportPreviewView> {
    let upload = read_upload(multipart).await?;
    let result = SalesInquiryImportProcess::new(state.db(), state.rbac())
        .preview(&actor, &id, upload.version, &upload.file_name, upload.bytes)
        .await?;
    Ok(ApiResponse::ok_with_data(result))
}

#[permission_macros::permission(
    group = "销售询价",
    group_desc = "询价需求与采购反馈协作",
    desc = "确认客户选品导入",
    resource = "sales_inquiry",
    action = "import"
)]
/// 重验商品资格、输入指纹和当前版本后确认导入。
/// # 参数
/// `state`、`actor` 为当前应用及账号，`id` 为询价，`request` 为原预览与稳定命令。
/// # 返回
/// 返回已写入草稿及版本，同键重试返回原结果。
/// # 错误
/// 预览、授权、版本、商品资格或幂等载荷冲突时整体拒绝。
pub async fn sales_import_confirm(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(request): Json<ConfirmInquiryImportCommand>,
) -> Result<InquiryImportResult> {
    Ok(ApiResponse::ok_with_data(
        SalesInquiryImportProcess::new(state.db(), state.rbac()).confirm(&actor, &id, request).await?,
    ))
}

struct WorkbookUpload {
    version: u64,
    file_name: String,
    bytes: Vec<u8>,
}

async fn read_upload(mut multipart: Multipart) -> std::result::Result<WorkbookUpload, Error> {
    let mut file = None;
    let mut version = None;
    while let Some(mut field) = multipart.next_field().await.map_err(upload_error)? {
        match field.name() {
            Some("expected_version") if version.is_none() => {
                let bytes = read_field(&mut field, 20).await?;
                version = Some(
                    std::str::from_utf8(&bytes)
                        .ok()
                        .and_then(|text| text.parse::<u64>().ok())
                        .filter(|value| *value > 0)
                        .ok_or_else(|| Error::BadRequest("请提供有效的询价版本".into()))?,
                );
            },
            Some("file") if file.is_none() => {
                let name = field
                    .file_name()
                    .ok_or_else(|| Error::BadRequest("请上传 Excel 工作簿".into()))?
                    .to_string();
                validate_workbook_type(&name, field.content_type())?;
                let bytes = read_field(&mut field, INQUIRY_IMPORT_MAX_BYTES).await?;
                file = Some((name, bytes));
            },
            _ => return Err(Error::BadRequest("导入只接受一个工作簿和一个询价版本".into())),
        }
    }
    let (file_name, bytes) = file.ok_or_else(|| Error::BadRequest("请上传 Excel 工作簿".into()))?;
    Ok(WorkbookUpload {
        version: version.ok_or_else(|| Error::BadRequest("请提供询价版本".into()))?,
        file_name,
        bytes,
    })
}

pub(super) async fn read_field(
    field: &mut axum::extract::multipart::Field<'_>,
    limit: usize,
) -> std::result::Result<Vec<u8>, Error> {
    let mut bytes = Vec::new();
    while let Some(chunk) = field.chunk().await.map_err(upload_error)? {
        if bytes.len().checked_add(chunk.len()).is_none_or(|length| length > limit) {
            return Err(Error::BadRequest("导入文件或字段超过大小限制，工作簿最多 5 MiB".into()));
        }
        bytes.extend_from_slice(&chunk);
    }
    if bytes.is_empty() {
        return Err(Error::BadRequest("导入文件或字段不能为空".into()));
    }
    Ok(bytes)
}

pub(super) fn validate_workbook_type(
    name: &str,
    content_type: Option<&str>,
) -> std::result::Result<(), Error> {
    if name.chars().count() > 255
        || !name.to_ascii_lowercase().ends_with(".xlsx")
        || !matches!(
            content_type,
            Some(
                "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"
                    | "application/octet-stream"
                    | "application/zip"
            )
        )
    {
        return Err(Error::BadRequest("请上传 .xlsx 格式的 Excel 工作簿".into()));
    }
    Ok(())
}
pub(super) fn upload_error(_: axum::extract::multipart::MultipartError) -> Error {
    Error::BadRequest("上传内容无法读取，请重新选择工作簿".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn workbook_upload_accepts_xlsx_and_rejects_other_types() {
        assert!(
            validate_workbook_type(
                "客户选品.xlsx",
                Some("application/vnd.openxmlformats-officedocument.spreadsheetml.sheet")
            )
            .is_ok()
        );
        assert!(validate_workbook_type("客户选品.XLSX", Some("application/octet-stream")).is_ok());
        assert!(validate_workbook_type("选品.xls", Some("application/octet-stream")).is_err());
        assert!(validate_workbook_type("选品.xlsx", Some("text/html")).is_err());
    }
}
