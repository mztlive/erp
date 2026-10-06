//! 静态 PDF 资料的严格结构检查；内容检查不登记病毒扫描通过。

use std::collections::{BTreeMap, HashSet};

use lopdf::content::Content;
use lopdf::{Dictionary, Document, LoadOptions, Object, ObjectId, Stream};

use super::assets::decode_image_with_budget;
use crate::core::errors::Error;
use crate::core::upload::MAX_UPLOAD_FILE_BYTES;

const MAX_PDF_OBJECTS: usize = 10_000;
const MAX_PDF_NODES: usize = 100_000;
const MAX_PDF_DEPTH: usize = 50;
const MAX_PDF_PAGES: usize = 500;
const MAX_PDF_DECODE_BYTES: usize = 32 * 1024 * 1024;
const MAX_PDF_STREAM_BYTES: usize = 8 * 1024 * 1024;

/// 检查完整静态 PDF、全部对象和有界解压内容，拒绝不能检查的文件。
/// # 参数
/// 已完成大小、扩展名和 MIME 一致性校验的原始 PDF 字节。
/// # 返回
/// 返回真实检查到的页面数。
/// # 错误
/// 损坏、加密、主动行为、嵌入文件、未知过滤器或资源预算超限时拒绝。
pub(super) fn inspect_pdf(bytes: &[u8]) -> Result<u32, Error> {
    preflight(bytes)?;
    let document = Document::load_mem_with_options(
        bytes,
        LoadOptions {
            strict: true,
            max_decompressed_size: Some(MAX_PDF_STREAM_BYTES),
            ..LoadOptions::default()
        },
    )
    .map_err(|_| invalid_pdf())?;
    if document.is_encrypted() || document.was_encrypted() || document.objects.len() > MAX_PDF_OBJECTS {
        return Err(invalid_pdf());
    }
    let mut inspection = PdfInspection::new();
    inspection.object(&Object::Dictionary(document.trailer.clone()), &document, 0)?;
    for (id, object) in &document.objects {
        inspection.object(object, &document, 0)?;
        if let Object::Stream(stream) = object {
            let content = inspection.decode_stream(stream)?;
            inspection.streams.insert(*id, content);
        }
    }
    inspection.pages(&document)
}

/// 载入前拒绝会被解析器提前解压的对象流和交叉引用流，限流总开销。
fn preflight(bytes: &[u8]) -> Result<(), Error> {
    if bytes.is_empty()
        || bytes.len() > MAX_UPLOAD_FILE_BYTES
        || !bytes.starts_with(b"%PDF-")
        || !bytes.trim_ascii_end().ends_with(b"%%EOF")
    {
        return Err(invalid_pdf());
    }
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] != b'/' {
            index += 1;
            continue;
        }
        index += 1;
        let mut name = Vec::new();
        while index < bytes.len() && !is_delimiter(bytes[index]) && bytes[index].is_ascii_graphic() {
            if bytes[index] == b'#' {
                let value = bytes.get(index + 1..index + 3).and_then(|escaped| {
                    std::str::from_utf8(escaped).ok().and_then(|v| u8::from_str_radix(v, 16).ok())
                });
                if let Some(value) = value {
                    name.push(value);
                    index += 3;
                    continue;
                }
            }
            name.push(bytes[index]);
            index += 1;
        }
        if matches!(name.as_slice(), b"Encrypt" | b"ObjStm" | b"XRef") {
            return Err(invalid_pdf());
        }
    }
    Ok(())
}

fn is_delimiter(value: u8) -> bool {
    value.is_ascii_whitespace()
        || matches!(value, b'(' | b')' | b'<' | b'>' | b'[' | b']' | b'{' | b'}' | b'/' | b'%')
}

fn forbidden_name(name: &[u8]) -> bool {
    matches!(
        name,
        b"A" | b"AA"
            | b"OpenAction"
            | b"JS"
            | b"JavaScript"
            | b"Launch"
            | b"URI"
            | b"GoToR"
            | b"GoToE"
            | b"SubmitForm"
            | b"ImportData"
            | b"Rendition"
            | b"RichMedia"
            | b"RichMediaContent"
            | b"RichMediaSettings"
            | b"Movie"
            | b"Sound"
            | b"EmbeddedFiles"
            | b"EmbeddedFile"
            | b"Filespec"
            | b"EF"
            | b"AF"
            | b"XFA"
            | b"AcroForm"
            | b"Encrypt"
            | b"ObjStm"
            | b"XRef"
            | b"Crypt"
            | b"3D"
            | b"3DD"
            | b"PS"
            | b"PA"
            | b"PresSteps"
            | b"FFilter"
            | b"FDecodeParms"
    )
}

struct PdfInspection {
    nodes: usize,
    remaining: usize,
    streams: BTreeMap<ObjectId, Vec<u8>>,
}

impl PdfInspection {
    fn new() -> Self {
        Self { nodes: 0, remaining: MAX_PDF_DECODE_BYTES, streams: BTreeMap::new() }
    }

    fn object(&mut self, object: &Object, document: &Document, depth: usize) -> Result<(), Error> {
        self.nodes += 1;
        if self.nodes > MAX_PDF_NODES || depth > MAX_PDF_DEPTH {
            return Err(invalid_pdf());
        }
        match object {
            Object::Name(name) if forbidden_name(name) => return Err(invalid_pdf()),
            Object::Reference(id) if !document.objects.contains_key(id) => return Err(invalid_pdf()),
            Object::Array(items) => {
                for item in items {
                    self.object(item, document, depth + 1)?;
                }
            },
            Object::Dictionary(dictionary) => self.dictionary(dictionary, document, depth + 1)?,
            Object::Stream(stream) => self.dictionary(&stream.dict, document, depth + 1)?,
            _ => {},
        }
        Ok(())
    }

    fn dictionary(
        &mut self,
        dictionary: &Dictionary,
        document: &Document,
        depth: usize,
    ) -> Result<(), Error> {
        for (key, object) in dictionary.iter() {
            if forbidden_name(key) {
                return Err(invalid_pdf());
            }
            self.object(object, document, depth)?;
        }
        Ok(())
    }

    fn decode_stream(&mut self, stream: &Stream) -> Result<Vec<u8>, Error> {
        if stream.dict.has(b"F") {
            return Err(invalid_pdf()); // 外部文件流不能在本次上传中完成真实内容检查。
        }
        let budget = self.remaining.min(MAX_PDF_STREAM_BYTES);
        let filters = if stream.dict.has(b"Filter") {
            stream.filters().map_err(|_| invalid_pdf())?
        } else {
            Vec::new()
        };
        if filters == [b"DCTDecode".as_slice()] {
            let (width, height) = decode_image_with_budget(
                &stream.content,
                "image/jpeg",
                u64::try_from(budget).map_err(|_| invalid_pdf())?,
            )?;
            if stream.dict.get(b"Width").and_then(Object::as_i64).ok() != Some(i64::from(width))
                || stream.dict.get(b"Height").and_then(Object::as_i64).ok() != Some(i64::from(height))
            {
                return Err(invalid_pdf());
            }
            let size =
                usize::try_from(u64::from(width) * u64::from(height) * 4).map_err(|_| invalid_pdf())?;
            self.remaining = self.remaining.checked_sub(size).ok_or_else(invalid_pdf)?;
            return Ok(Vec::new());
        }
        if filters.len() > 3
            || filters.iter().any(|filter| {
                !matches!(*filter, b"FlateDecode" | b"ASCII85Decode" | b"ASCIIHexDecode" | b"RunLengthDecode")
            })
        {
            return Err(invalid_pdf());
        }
        if stream.dict.has(b"DecodeParms")
            && !matches!(stream.dict.get(b"DecodeParms"), Ok(Object::Dictionary(_) | Object::Null))
        {
            return Err(invalid_pdf());
        }
        let decoded = stream.get_plain_content_with_limit(budget).map_err(|_| invalid_pdf())?;
        self.remaining = self.remaining.checked_sub(decoded.len()).ok_or_else(invalid_pdf)?;
        Ok(decoded)
    }

    fn pages(&self, document: &Document) -> Result<u32, Error> {
        let root = document
            .catalog()
            .map_err(|_| invalid_pdf())?
            .get(b"Pages")
            .and_then(Object::as_reference)
            .map_err(|_| invalid_pdf())?;
        let mut stack = vec![(root, None, 0)];
        let mut seen = HashSet::new();
        let mut pages = 0usize;
        let expected_pages = document
            .get_dictionary(root)
            .map_err(|_| invalid_pdf())?
            .get(b"Count")
            .and_then(Object::as_i64)
            .map_err(|_| invalid_pdf())?;
        while let Some((id, parent, depth)) = stack.pop() {
            if !seen.insert(id) || seen.len() > MAX_PDF_OBJECTS || depth > MAX_PDF_DEPTH {
                return Err(invalid_pdf());
            }
            let dictionary = document.get_dictionary(id).map_err(|_| invalid_pdf())?;
            if let Some(parent) = parent
                && dictionary.get(b"Parent").and_then(Object::as_reference).ok() != Some(parent)
            {
                return Err(invalid_pdf());
            }
            match dictionary.get_type().map_err(|_| invalid_pdf())? {
                b"Pages" => {
                    let kids =
                        dictionary.get(b"Kids").and_then(Object::as_array).map_err(|_| invalid_pdf())?;
                    if kids.is_empty() || kids.len() > MAX_PDF_PAGES {
                        return Err(invalid_pdf());
                    }
                    for kid in kids {
                        stack.push((kid.as_reference().map_err(|_| invalid_pdf())?, Some(id), depth + 1));
                    }
                },
                b"Page" => {
                    pages += 1;
                    if pages > MAX_PDF_PAGES {
                        return Err(invalid_pdf());
                    }
                    self.page_content(dictionary)?;
                },
                _ => return Err(invalid_pdf()),
            }
        }
        if pages == 0 || i64::try_from(pages).ok() != Some(expected_pages) {
            return Err(invalid_pdf());
        }
        u32::try_from(pages).map_err(|_| invalid_pdf())
    }

    fn page_content(&self, page: &Dictionary) -> Result<(), Error> {
        let contents = match page.get(b"Contents") {
            Ok(Object::Reference(id)) => vec![*id],
            Ok(Object::Array(items)) => items
                .iter()
                .map(|item| item.as_reference().map_err(|_| invalid_pdf()))
                .collect::<Result<Vec<_>, _>>()?,
            Err(_) => return Ok(()), // 合法空白页。
            _ => return Err(invalid_pdf()),
        };
        let mut combined = Vec::new();
        for id in contents {
            let content = self.streams.get(&id).ok_or_else(invalid_pdf)?;
            if combined.len().saturating_add(content.len()) > MAX_PDF_STREAM_BYTES {
                return Err(invalid_pdf());
            }
            combined.extend_from_slice(content);
            combined.push(b'\n');
        }
        Content::decode_strict(&combined).map_err(|_| invalid_pdf())?;
        Ok(())
    }
}

fn invalid_pdf() -> Error {
    Error::BadRequest("PDF 资料须为完整静态文件；加密、主动内容、嵌入附件或无法检查的格式不予接收".into())
}

#[cfg(test)]
mod tests {
    use lopdf::xref::XrefType;
    use lopdf::{Stream, dictionary};

    use super::*;

    fn document() -> Document {
        let mut doc = Document::with_version("1.7");
        doc.reference_table.cross_reference_type = XrefType::CrossReferenceTable;
        let pages = doc.new_object_id();
        let content = doc.add_object(Stream::new(dictionary! {}, b"q Q".to_vec()));
        let page = doc.add_object(dictionary! {"Type"=>"Page", "Parent"=>pages,
        "MediaBox"=>vec![0.into(),0.into(),100.into(),100.into()], "Contents"=>content});
        doc.objects.insert(
            pages,
            Object::Dictionary(dictionary! {"Type"=>"Pages", "Count"=>1, "Kids"=>vec![page.into()]}),
        );
        let catalog = doc.add_object(dictionary! {"Type"=>"Catalog", "Pages"=>pages});
        doc.trailer.set("Root", catalog);
        doc
    }

    fn bytes(mut doc: Document) -> Vec<u8> {
        let mut bytes = Vec::new();
        doc.save_to(&mut bytes).unwrap();
        bytes
    }

    fn raw_pdf(catalog_extra: &str) -> Vec<u8> {
        let objects = [
            format!("<< /Type /Catalog /Pages 2 0 R {catalog_extra} >>"),
            "<< /Type /Pages /Count 1 /Kids [3 0 R] >>".into(),
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 100 100] >>".into(),
        ];
        let mut bytes = b"%PDF-1.7\n".to_vec();
        let mut offsets = Vec::new();
        for (index, object) in objects.iter().enumerate() {
            offsets.push(bytes.len());
            bytes.extend_from_slice(format!("{} 0 obj\n{object}\nendobj\n", index + 1).as_bytes());
        }
        let xref = bytes.len();
        bytes.extend_from_slice(b"xref\n0 4\n0000000000 65535 f \n");
        for offset in offsets {
            bytes.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
        }
        bytes.extend_from_slice(
            format!("trailer\n<< /Size 4 /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n").as_bytes(),
        );
        bytes
    }

    #[test]
    fn actual_static_pdf_accepts_pages_and_rejects_invalid_truncated_or_encrypted_files() {
        let valid = bytes(document());
        preflight(&valid).unwrap();
        assert_eq!(inspect_pdf(&valid).unwrap(), 1);
        assert_eq!(inspect_pdf(&raw_pdf("")).unwrap(), 1);
        assert!(inspect_pdf(b"%PDF-1.7\n%%EOF").is_err());
        assert!(inspect_pdf(&valid[..valid.len() / 2]).is_err());
        let mut encrypted = document();
        let encryption = encrypted.add_object(dictionary! {"Filter"=>"Standard"});
        encrypted.trailer.set("Encrypt", encryption);
        assert!(inspect_pdf(&bytes(encrypted)).is_err());
        assert!(preflight(b"%PDF-1.7\n/Encr#79pt 1 0 R\n%%EOF").is_err());
        let mut compressed = document();
        let stream = compressed.objects.values_mut().find_map(|object| object.as_stream_mut().ok()).unwrap();
        stream.set_content(b"q Q ".repeat(500));
        stream.compress().unwrap();
        assert_eq!(inspect_pdf(&bytes(compressed)).unwrap(), 1);
        let mut illustrated = document();
        illustrated.add_object(Stream::new(
            dictionary! {"Type"=>"XObject", "Subtype"=>"Image",
            "Width"=>2, "Height"=>2, "Filter"=>"DCTDecode"},
            include_bytes!("fixtures/black-2x2.jpg").to_vec(),
        ));
        assert_eq!(inspect_pdf(&bytes(illustrated)).unwrap(), 1);
    }

    #[test]
    fn parsed_names_reject_actions_embedded_files_and_active_forms_even_when_encoded() {
        for key in ["OpenAction", "AA", "JavaScript", "EmbeddedFiles", "XFA", "AcroForm", "URI"] {
            let mut doc = document();
            let root = doc.trailer.get(b"Root").unwrap().as_reference().unwrap();
            doc.get_object_mut(root).unwrap().as_dict_mut().unwrap().set(key, Object::Null);
            assert!(inspect_pdf(&bytes(doc)).is_err(), "{key}");
        }
        let mut doc = document();
        doc.add_object(dictionary! {"Type"=>"Filespec", "EF"=>dictionary! {}});
        assert!(inspect_pdf(&bytes(doc)).is_err());
        let escaped_action = raw_pdf("/Open#41ction << /S /Java#53cript /#4A#53 (alert\\(1\\)) >>");
        assert!(
            Document::load_mem_with_options(
                &escaped_action,
                LoadOptions { strict: true, ..LoadOptions::default() }
            )
            .is_ok(),
            "编码名称仍为结构完整的 PDF"
        );
        assert!(inspect_pdf(&escaped_action).is_err());
    }

    #[test]
    fn page_cycles_invalid_references_and_unknown_filters_are_rejected() {
        let mut doc = document();
        let root = doc.catalog().unwrap().get(b"Pages").unwrap().as_reference().unwrap();
        doc.get_object_mut(root).unwrap().as_dict_mut().unwrap().set("Kids", vec![Object::Reference(root)]);
        assert!(inspect_pdf(&bytes(doc)).is_err());
        let mut doc = document();
        doc.add_object(dictionary! {"Missing"=>Object::Reference((99_999,0))});
        assert!(inspect_pdf(&bytes(doc)).is_err());
        let mut doc = document();
        doc.add_object(Stream::new(dictionary! {"Filter"=>"UncheckableDecode"}, b"unknown".to_vec()));
        assert!(inspect_pdf(&bytes(doc)).is_err());
    }

    #[test]
    fn stream_decode_limits_apply_to_real_decompression_and_total_budget() {
        let mut stream = Stream::new(dictionary! {}, vec![b'A'; MAX_PDF_STREAM_BYTES + 1]);
        stream.compress().unwrap();
        assert!(PdfInspection::new().decode_stream(&stream).is_err());
        let stream = Stream::new(dictionary! {}, vec![b'A'; 32]);
        let mut inspection = PdfInspection::new();
        inspection.remaining = 31;
        assert!(inspection.decode_stream(&stream).is_err());
        let mut inspection = PdfInspection::new();
        inspection.remaining = 64;
        assert_eq!(inspection.decode_stream(&stream).unwrap().len(), 32);
        assert_eq!(inspection.decode_stream(&stream).unwrap().len(), 32);
        assert!(inspection.decode_stream(&stream).is_err());
    }

    #[test]
    fn bounded_parser_and_inspection_reject_deep_or_oversized_object_graphs() {
        let nested = format!("/Deep {}0{}", "[".repeat(1_000), "]".repeat(1_000));
        assert!(inspect_pdf(&raw_pdf(&nested)).is_err());
        let nested = format!("/Deep {}0{}", "[".repeat(MAX_PDF_DEPTH + 1), "]".repeat(MAX_PDF_DEPTH + 1));
        assert!(inspect_pdf(&raw_pdf(&nested)).is_err());
        let mut doc = document();
        doc.add_object(Object::Array(vec![Object::Null; MAX_PDF_NODES + 1]));
        assert!(inspect_pdf(&bytes(doc)).is_err());
        let mut doc = document();
        for _ in 0..MAX_PDF_OBJECTS {
            doc.add_object(Object::Null);
        }
        assert!(inspect_pdf(&bytes(doc)).is_err());
    }
}
