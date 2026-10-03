//! Word 模板 multipart 的独立协议校验，不扩大现有业务凭证允许的格式。

use axum::extract::Multipart;
use erp_contract::dto::template::CreateTemplateRequest;
use erp_contract::entity::template_docx::{DOCX_MIME, MAX_TEMPLATE_BYTES};

use crate::core::errors::Error;

pub(super) struct TemplateFile {
    pub name: String,
    pub bytes: Vec<u8>,
}

pub(super) async fn extract(
    multipart: &mut Multipart,
) -> std::result::Result<(CreateTemplateRequest, TemplateFile), Error> {
    let mut command = None;
    let mut file = None;
    while let Some(mut field) = multipart.next_field().await.map_err(|_| bad("上传表单无效"))? {
        match field.name() {
            Some("command") if command.is_none() && field.file_name().is_none() => {
                let bytes = field.bytes().await.map_err(|_| bad("模板资料读取失败"))?;
                if bytes.len() > 8192 {
                    return Err(bad("模板资料过长"));
                }
                command = Some(serde_json::from_slice(&bytes).map_err(|_| bad("模板资料格式无效"))?);
            },
            Some("file") if file.is_none() => {
                let name = field.file_name().ok_or_else(|| bad("缺少模板文件名"))?.to_string();
                validate_file(&name, field.content_type().unwrap_or_default())?;
                let mut bytes = Vec::new();
                while let Some(chunk) = field.chunk().await.map_err(|_| bad("Word 模板读取失败"))? {
                    if bytes.len().saturating_add(chunk.len()) > MAX_TEMPLATE_BYTES {
                        return Err(bad("Word 模板不能超过 20 MB"));
                    }
                    bytes.extend_from_slice(&chunk);
                }
                file = Some(TemplateFile { name, bytes });
            },
            _ => return Err(bad("模板上传仅允许一份 DOCX 和一份模板资料")),
        }
    }
    Ok((command.ok_or_else(|| bad("缺少模板资料"))?, file.ok_or_else(|| bad("请选择 Word 模板"))?))
}

fn bad(message: &str) -> Error {
    Error::BadRequest(message.into())
}

fn validate_file(name: &str, mime: &str) -> std::result::Result<(), Error> {
    if !name.to_ascii_lowercase().ends_with(".docx") || !mime.eq_ignore_ascii_case(DOCX_MIME) {
        return Err(bad("合同模板只支持 Word（DOCX），请另存为 DOCX 后上传"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::extract::FromRequest;
    use axum::http::Request;

    use super::*;

    #[test]
    fn rejects_old_word_pdf_and_forged_mime() {
        assert!(validate_file("合同.DOCX", DOCX_MIME).is_ok());
        assert!(validate_file("合同.doc", DOCX_MIME).is_err());
        assert!(validate_file("合同.pdf", "application/pdf").is_err());
        assert!(validate_file("合同.docx", "application/pdf").is_err());
    }

    #[tokio::test]
    async fn accepts_either_order_and_rejects_duplicate_fields() {
        let command = "--x\r\nContent-Disposition: form-data; name=\"command\"\r\n\r\n{\"name\":\"合同\",\"company_id\":\"company\",\"group\":\"FSY\"}\r\n";
        let file = format!(
            "--x\r\nContent-Disposition: form-data; name=\"file\"; filename=\"contract.docx\"\r\nContent-Type: {DOCX_MIME}\r\n\r\nzip bytes\r\n"
        );
        for body in [format!("{command}{file}--x--\r\n"), format!("{file}{command}--x--\r\n")] {
            let request = Request::builder()
                .header("content-type", "multipart/form-data; boundary=x")
                .body(Body::from(body))
                .unwrap();
            let mut multipart = Multipart::from_request(request, &()).await.unwrap();
            let (request, file) = extract(&mut multipart).await.unwrap();
            assert_eq!(request.name, "合同");
            assert_eq!(file.bytes, b"zip bytes");
        }
        let request = Request::builder()
            .header("content-type", "multipart/form-data; boundary=x")
            .body(Body::from(format!("{command}{command}{file}--x--\r\n")))
            .unwrap();
        let mut multipart = Multipart::from_request(request, &()).await.unwrap();
        assert!(extract(&mut multipart).await.is_err());
    }
}
