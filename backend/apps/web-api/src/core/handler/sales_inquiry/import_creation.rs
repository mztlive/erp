//! 列表页选择客户和工作簿，校验后原子创建询价草稿。

use application_core::AuditActor;
use axum::Extension;
use axum::extract::{Multipart, State};
use erp_processes::sales_inquiry_import::{
    CreateInquiryImportCommand, InquiryImportPreviewView, SalesInquiryImportProcess,
};
use erp_sales::entity::sales_inquiry_import::{INQUIRY_IMPORT_MAX_BYTES, InquiryImportResult};

use super::import::{read_field, upload_error, validate_workbook_type};
use crate::app_state::AppState;
use crate::core::errors::{Error, Result};
use crate::core::extractor::Json;
use crate::core::response::ApiResponse;

#[permission_macros::permission(
    group = "销售询价",
    group_desc = "询价需求与采购反馈协作",
    desc = "预览导入新建询价",
    resource = "sales_inquiry",
    action = "import"
)]
/// 校验客户绑定的新建文件，不创建空草稿。
/// # 参数
/// multipart 必须包含一个 customer_id 和一个 file，身份取当前账号。
/// # 返回
/// 返回逐行校验预览。
/// # 错误
/// 文件、字段、客户或创建与导入权限无效时拒绝。
pub async fn creation_preview(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    multipart: Multipart,
) -> Result<InquiryImportPreviewView> {
    let (customer, name, bytes) = read_upload(multipart).await?;
    Ok(ApiResponse::ok_with_data(
        SalesInquiryImportProcess::new(state.db(), state.rbac())
            .preview_creation(&actor, &customer, &name, bytes)
            .await?,
    ))
}

#[permission_macros::permission(
    group = "销售询价",
    group_desc = "询价需求与采购反馈协作",
    desc = "导入新建询价",
    resource = "sales_inquiry",
    action = "import"
)]
/// 从原预览一次性创建询价和配送需求。
/// # 参数
/// `request` 仅包含客户、预览、指纹、合并确认及稳定命令号。
/// # 返回
/// 返回草稿身份和版本；重放返回首次结果。
/// # 错误
/// 预览、权限、商品或客户失效时整体拒绝。
pub async fn creation_confirm(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Json(request): Json<CreateInquiryImportCommand>,
) -> Result<InquiryImportResult> {
    Ok(ApiResponse::ok_with_data(
        SalesInquiryImportProcess::new(state.db(), state.rbac()).create_from_preview(&actor, request).await?,
    ))
}

async fn read_upload(mut multipart: Multipart) -> std::result::Result<(String, String, Vec<u8>), Error> {
    let mut customer = None;
    let mut file = None;
    while let Some(mut field) = multipart.next_field().await.map_err(upload_error)? {
        match field.name() {
            Some("customer_id") if customer.is_none() => {
                let bytes = read_field(&mut field, 128).await?;
                customer = Some(parse_customer(&bytes)?);
            },
            Some("file") if file.is_none() => {
                let name = field
                    .file_name()
                    .ok_or_else(|| Error::BadRequest("请上传 Excel 工作簿".into()))?
                    .to_string();
                validate_workbook_type(&name, field.content_type())?;
                file = Some((name, read_field(&mut field, INQUIRY_IMPORT_MAX_BYTES).await?));
            },
            _ => return Err(Error::BadRequest("只接受一个客户和一个工作簿".into())),
        }
    }
    let customer = customer.ok_or_else(|| Error::BadRequest("请选择客户".into()))?;
    let (name, bytes) = file.ok_or_else(|| Error::BadRequest("请上传 Excel 工作簿".into()))?;
    Ok((customer, name, bytes))
}

fn parse_customer(bytes: &[u8]) -> std::result::Result<String, Error> {
    let customer = std::str::from_utf8(bytes)
        .ok()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| Error::BadRequest("请选择有效客户".into()))?;
    Ok(customer.into())
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::extract::FromRequest;
    use axum::http::Request;

    use super::*;

    async fn upload(fields: &[(&str, Option<&str>, &str)]) -> Multipart {
        let mut body = String::new();
        for (name, file, value) in fields {
            body.push_str(&format!("--boundary\r\nContent-Disposition: form-data; name=\"{name}\""));
            if let Some(file) = file {
                body.push_str(&format!("; filename=\"{file}\"\r\nContent-Type: application/octet-stream"));
            }
            body.push_str(&format!("\r\n\r\n{value}\r\n"));
        }
        body.push_str("--boundary--\r\n");
        let request = Request::builder()
            .header("content-type", "multipart/form-data; boundary=boundary")
            .body(Body::from(body))
            .unwrap();
        Multipart::from_request(request, &()).await.unwrap()
    }

    #[tokio::test]
    async fn upload_accepts_one_customer_and_workbook_in_either_order() {
        let fields = [("customer_id", None, " customer "), ("file", Some("selected.xlsx"), "bytes")];
        let expected = ("customer".into(), "selected.xlsx".into(), b"bytes".to_vec());
        assert_eq!(read_upload(upload(&fields).await).await.unwrap(), expected);
        assert_eq!(read_upload(upload(&[fields[1], fields[0]]).await).await.unwrap(), expected);
    }

    #[tokio::test]
    async fn upload_rejects_missing_duplicate_extra_and_oversized_fields() {
        let customer = ("customer_id", None, "customer");
        let file = ("file", Some("selected.xlsx"), "bytes");
        let oversized = "a".repeat(129);
        for fields in [
            vec![customer],
            vec![file],
            vec![customer, customer, file],
            vec![customer, file, file],
            vec![customer, file, ("unexpected", None, "value")],
            vec![("customer_id", None, oversized.as_str()), file],
            vec![customer, ("file", Some("selected.csv"), "bytes")],
        ] {
            assert!(read_upload(upload(&fields).await).await.is_err());
        }
    }

    #[test]
    fn customer_field_must_be_nonempty_utf8() {
        assert_eq!(parse_customer(b" customer ").unwrap(), "customer");
        assert!(parse_customer(b"  ").is_err());
        assert!(parse_customer(&[255]).is_err());
    }
}
