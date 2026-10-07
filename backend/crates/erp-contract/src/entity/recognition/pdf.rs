//! 合同文件的纯格式检查，不访问数据库或供应商。
use lopdf::{Document as PdfDocument, LoadOptions};

use super::ImportFailure;

/// 独立解析原 PDF 的真实页数。
/// # 参数
/// * `pdf` - 受控大小的原文件字节。
/// # 返回
/// 1 至 200 页。
/// # 错误
/// 损坏、加密、空文档或超限。
pub fn pdf_page_count(pdf: &[u8]) -> Result<u32, ImportFailure> {
    let document = PdfDocument::load_mem_with_options(
        pdf,
        LoadOptions { strict: true, max_decompressed_size: Some(8 * 1024 * 1024), ..LoadOptions::default() },
    )
    .map_err(|_| ImportFailure::new("INVALID_PDF", "PDF 无法解析，请上传完整文件"))?;
    let pages = u32::try_from(document.get_pages().len()).unwrap_or(u32::MAX);
    if document.is_encrypted()
        || document.was_encrypted()
        || document.objects.len() > 10_000
        || !(1..=200).contains(&pages)
    {
        return Err(ImportFailure::new("INVALID_PDF", "请上传未加密且不超过 200 页的合同 PDF"));
    }
    Ok(pages)
}

#[cfg(test)]
mod tests {
    use lopdf::{Object, dictionary};

    use super::*;

    #[test]
    fn counts_actual_pages_and_rejects_empty_or_broken_pdf() {
        let mut doc = PdfDocument::with_version("1.5");
        let pages_id = doc.new_object_id();
        let page = doc.add_object(dictionary! { "Type" => "Page", "Parent" => pages_id, "MediaBox" => vec![0.into(), 0.into(), 100.into(), 100.into()] });
        doc.objects.insert(
            pages_id,
            Object::Dictionary(
                dictionary! { "Type" => "Pages", "Kids" => vec![Object::Reference(page)], "Count" => 1 },
            ),
        );
        let root = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
        doc.trailer.set("Root", root);
        let mut bytes = Vec::new();
        doc.save_to(&mut bytes).unwrap();
        assert_eq!(pdf_page_count(&bytes).unwrap(), 1);
        assert!(pdf_page_count(b"%PDF-invalid").is_err());
        assert!(pdf_page_count(&[]).is_err());
    }
}
