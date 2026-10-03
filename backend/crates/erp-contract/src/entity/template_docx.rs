//! 有界读取 DOCX，仅修改首页页眉关系和分节属性，正文保留原始字节。

use std::collections::BTreeMap;
use std::io::{Cursor, Read, Write};
use std::ops::Range;

use roxmltree::{Document, Node};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

use crate::error::{Error, Result};

const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
const R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const PKG: &str = "http://schemas.openxmlformats.org/package/2006/relationships";
pub const DOCX_MIME: &str = "application/vnd.openxmlformats-officedocument.wordprocessingml.document";
pub const MAX_TEMPLATE_BYTES: usize = 20 * 1024 * 1024;

type Package = BTreeMap<String, Vec<u8>>;

fn invalid() -> Error {
    Error::ValidationError("Word 模板无法处理，请重新另存为标准 DOCX 后上传".into())
}

/// 将合同号写入 Word 文档第一页右上角的独立首页页眉。
/// # 参数
/// * `bytes` - 原 DOCX 包。
/// * `number` - 领域生成的 ASCII 合同号。
/// # 返回
/// 可继续编辑及打印的 DOCX；正文、图片和后续页页眉保持原内容。
/// # 错误
/// 非标准 DOCX、超限压缩包或无有效分节信息时拒绝。
pub fn stamp(bytes: &[u8], number: &str) -> Result<Vec<u8>> {
    if number.is_empty()
        || number.len() > 32
        || !number.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-')
    {
        return Err(invalid());
    }
    let mut package = unpack(bytes)?;
    let document_xml = xml(&package, "word/document.xml")?.to_string();
    let document = parse(&document_xml)?;
    if document.root_element().tag_name().namespace() != Some(W) {
        return Err(invalid());
    }
    let sections = document
        .descendants()
        .filter(|n| {
            n.has_tag_name((W, "sectPr")) && !n.ancestors().any(|a| a.has_tag_name((W, "sectPrChange")))
        })
        .collect::<Vec<_>>();
    let first = *sections.first().ok_or_else(invalid)?;
    let source_id = reference(first, "headerReference", if title_page(first) { "first" } else { "default" });
    let headers = add_headers(&mut package, source_id, number)?;
    let mut edits = Vec::new();
    let mut inherited_first: Option<&str> = None;
    let mut inherited_footer: Option<&str> = None;
    for (index, section) in sections.into_iter().enumerate() {
        let original_first = reference(section, "headerReference", "first");
        let original_footer = reference(section, "footerReference", "first");
        if index == 0 {
            edits.push((section.range(), first_section(&document_xml, section, &headers.number)?));
        } else {
            let header = original_first.is_none().then_some(inherited_first.unwrap_or(&headers.blank));
            let footer = original_footer.is_none().then_some(inherited_footer.unwrap_or(&headers.footer));
            edits.push((section.range(), section_references(&document_xml, section, &[], header, footer)?));
        }
        inherited_first = original_first.or(inherited_first);
        inherited_footer = original_footer.or(inherited_footer);
    }
    package.insert("word/document.xml".into(), replace(&document_xml, edits)?.into_bytes());
    pack(package)
}

/// 每个 ZIP 条目和累计展开体积都有限；禁止重名、路径穿越及宏包。
fn unpack(bytes: &[u8]) -> Result<Package> {
    if bytes.is_empty() || bytes.len() > MAX_TEMPLATE_BYTES {
        return Err(invalid());
    }
    let mut archive = ZipArchive::new(Cursor::new(bytes)).map_err(|_| invalid())?;
    if archive.len() > 2048 {
        return Err(invalid());
    }
    let mut package = Package::new();
    let mut total = 0_u64;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).map_err(|_| invalid())?;
        let name = entry.name().to_string();
        if name.starts_with('/')
            || name.contains('\\')
            || name.split('/').any(|p| p == "..")
            || name.to_lowercase().contains("vbaproject")
            || name.starts_with("_xmlsignatures/")
        {
            return Err(invalid());
        }
        total = total.checked_add(entry.size()).ok_or_else(invalid)?;
        if entry.size() > 32 * 1024 * 1024 || total > 100 * 1024 * 1024 {
            return Err(invalid());
        }
        if (name.ends_with(".xml") || name.ends_with(".rels")) && entry.size() > 10 * 1024 * 1024 {
            return Err(invalid());
        }
        let mut content = Vec::new();
        (&mut entry).take(32 * 1024 * 1024 + 1).read_to_end(&mut content).map_err(|_| invalid())?;
        if content.len() > 32 * 1024 * 1024 || package.insert(name, content).is_some() {
            return Err(invalid());
        }
    }
    let types = parse(xml(&package, "[Content_Types].xml")?)?;
    if !types.descendants().any(|n| {
        n.attribute("PartName") == Some("/word/document.xml")
            && n.attribute("ContentType")
                == Some("application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml")
    }) {
        return Err(invalid());
    }
    let root = parse(xml(&package, "_rels/.rels")?)?;
    if !root.descendants().any(|n| {
        n.attribute("Type") == Some(format!("{R}/officeDocument").as_str())
            && n.attribute("Target") == Some("word/document.xml")
            && n.attribute("TargetMode") != Some("External")
    }) {
        return Err(invalid());
    }
    Ok(package)
}

fn pack(package: Package) -> Result<Vec<u8>> {
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    for (name, bytes) in package {
        writer.start_file(name, options).map_err(|_| invalid())?;
        writer.write_all(&bytes).map_err(|_| invalid())?;
    }
    Ok(writer.finish().map_err(|_| invalid())?.into_inner())
}

fn xml<'a>(package: &'a Package, name: &str) -> Result<&'a str> {
    let bytes = package.get(name).ok_or_else(invalid)?;
    if bytes.len() > 10 * 1024 * 1024 {
        return Err(invalid());
    }
    std::str::from_utf8(bytes).map_err(|_| invalid())
}

fn parse(source: &str) -> Result<Document<'_>> {
    Document::parse(source).map_err(|_| invalid())
}

fn reference<'a>(section: Node<'a, 'a>, tag: &str, kind: &str) -> Option<&'a str> {
    section
        .children()
        .find(|n| n.has_tag_name((W, tag)) && n.attribute((W, "type")) == Some(kind))
        .and_then(|n| n.attribute((R, "id")))
}

fn title_page(section: Node<'_, '_>) -> bool {
    section
        .children()
        .find(|n| n.has_tag_name((W, "titlePg")))
        .is_some_and(|n| !matches!(n.attribute((W, "val")), Some("0" | "false" | "off")))
}

fn header_reference(id: &str) -> String {
    let id = escape_attribute(id);
    format!("<w:headerReference xmlns:w=\"{W}\" xmlns:r=\"{R}\" w:type=\"first\" r:id=\"{id}\"/>")
}

fn footer_reference(id: &str) -> String {
    let id = escape_attribute(id);
    format!("<w:footerReference xmlns:w=\"{W}\" xmlns:r=\"{R}\" w:type=\"first\" r:id=\"{id}\"/>")
}

fn escape_attribute(value: &str) -> String {
    value.replace('&', "&amp;").replace('"', "&quot;").replace('<', "&lt;").replace('>', "&gt;")
}

fn first_section(source: &str, section: Node<'_, '_>, id: &str) -> Result<String> {
    let mut remove = vec![("headerReference", "first"), ("titlePg", "")];
    let mut footer = None;
    if !title_page(section) {
        remove.push(("footerReference", "first"));
        footer = reference(section, "footerReference", "default");
    }
    let result = section_references(source, section, &remove, Some(id), footer)?;
    // titlePg 的 schema 顺序在 docGrid、printerSettings 前。
    // 原文可以使用额外前缀，顺序定位沿用原分节节点而非重新解析片段。
    let tail = section.children().find(|n| {
        n.is_element()
            && matches!(
                n.tag_name().name(),
                "textDirection" | "bidi" | "rtlGutter" | "docGrid" | "printerSettings" | "sectPrChange"
            )
    });
    let anchor = tail.map(|n| &source[n.range()]);
    let offset =
        anchor.and_then(|raw| result.find(raw)).unwrap_or_else(|| result.rfind("</").unwrap_or(result.len()));
    Ok(format!("{}<w:titlePg xmlns:w=\"{W}\"/>{}", &result[..offset], &result[offset..]))
}

/// 保持 sectPr 中全部页眉引用先于全部页脚引用的 schema 顺序。
fn section_references(
    source: &str,
    section: Node<'_, '_>,
    remove: &[(&str, &str)],
    header: Option<&str>,
    footer: Option<&str>,
) -> Result<String> {
    let insertion = header.map(header_reference).unwrap_or_default();
    let result = edit_section(source, section, remove, &insertion)?;
    let Some(footer) = footer else {
        return Ok(result);
    };
    let mut offset = result.find('>').ok_or_else(invalid)? + 1 + insertion.len();
    for node in section.children().filter(|n| n.has_tag_name((W, "headerReference"))) {
        if remove.iter().any(|(tag, kind)| {
            *tag == "headerReference" && (kind.is_empty() || node.attribute((W, "type")) == Some(*kind))
        }) {
            continue;
        }
        let raw = &source[node.range()];
        if let Some(start) = result.find(raw) {
            offset = offset.max(start + raw.len());
        }
    }
    Ok(format!("{}{}{}", &result[..offset], footer_reference(footer), &result[offset..]))
}

/// 只改分节属性片段，不重新序列化正文及其命名空间属性。
fn edit_section(
    source: &str,
    section: Node<'_, '_>,
    remove: &[(&str, &str)],
    insertion: &str,
) -> Result<String> {
    let range = section.range();
    let fragment = &source[range.clone()];
    let mut edits = section
        .children()
        .filter(|n| {
            n.is_element()
                && remove.iter().any(|(tag, kind)| {
                    n.has_tag_name((W, *tag)) && (kind.is_empty() || n.attribute((W, "type")) == Some(*kind))
                })
        })
        .map(|n| ((n.range().start - range.start)..(n.range().end - range.start), String::new()))
        .collect::<Vec<_>>();
    let end = fragment.find('>').ok_or_else(invalid)?;
    if fragment[..end].trim_end().ends_with('/') {
        let opening = fragment[..end].trim_end().trim_end_matches('/');
        let name = opening[1..].split_whitespace().next().ok_or_else(invalid)?;
        return Ok(format!("{opening}>{insertion}</{name}>"));
    }
    edits.push(((end + 1)..(end + 1), insertion.into()));
    replace(fragment, edits)
}

fn replace(source: &str, mut edits: Vec<(Range<usize>, String)>) -> Result<String> {
    // 同一位置先插入，再删除原节点，避免首页引用恰好是首个子节点时误判重叠。
    edits.sort_by_key(|(range, _)| (range.start, range.end));
    let mut output = String::new();
    let mut cursor = 0;
    for (range, value) in edits {
        if range.start < cursor || range.end > source.len() {
            return Err(invalid());
        }
        output.push_str(&source[cursor..range.start]);
        output.push_str(&value);
        cursor = range.end;
    }
    output.push_str(&source[cursor..]);
    Ok(output)
}

fn insert_root(source: &str, element: &str) -> Result<String> {
    let document = parse(source)?;
    let root = document.root_element();
    edit_section(source, root, &[], element)
        .map(|body| format!("{}{}{}", &source[..root.range().start], body, &source[root.range().end..]))
}

struct AddedHeaders {
    number: String,
    blank: String,
    footer: String,
}

fn add_headers(package: &mut Package, source_id: Option<&str>, number: &str) -> Result<AddedHeaders> {
    let rels_path = "word/_rels/document.xml.rels";
    let rels = xml(package, rels_path)?.to_string();
    let relationships = parse(&rels)?;
    let mut header = format!("<w:hdr xmlns:w=\"{W}\"><w:p/></w:hdr>");
    let source = source_id
        .map(|id| relationships.descendants().find(|n| n.attribute("Id") == Some(id)).ok_or_else(invalid))
        .transpose()?;
    let target = source.map(|n| n.attribute("Target").ok_or_else(invalid)).transpose()?;
    if let Some(target) = target {
        if source.is_some_and(|n| {
            n.attribute("TargetMode") == Some("External")
                || n.attribute("Type") != Some(&format!("{R}/header"))
        }) || target.contains('/')
            || target.contains('\\')
        {
            return Err(invalid());
        }
        header = xml(package, &format!("word/{target}"))?.to_string();
    }
    let suffix = available_header_suffix(package, &relationships)?;
    let (name, blank) = (format!("erp-number-{suffix}.xml"), format!("erp-blank-{suffix}.xml"));
    let (id, blank_id) = (format!("rIdErpNumber{suffix}"), format!("rIdErpBlank{suffix}"));
    let paragraph = format!(
        "<w:p xmlns:w=\"{W}\"><w:pPr><w:jc w:val=\"right\"/><w:spacing w:before=\"0\" w:after=\"0\"/></w:pPr><w:r><w:rPr><w:rFonts w:ascii=\"Arial\" w:hAnsi=\"Arial\"/><w:sz w:val=\"20\"/><w:color w:val=\"000000\"/></w:rPr><w:t>{number}</w:t></w:r></w:p>"
    );
    package.insert(format!("word/{name}"), insert_root(&header, &paragraph)?.into_bytes());
    package.insert(format!("word/{blank}"), format!("<w:hdr xmlns:w=\"{W}\"><w:p/></w:hdr>").into_bytes());
    if let Some(target) = target
        && let Some(bytes) = package.get(&format!("word/_rels/{target}.rels")).cloned()
    {
        package.insert(format!("word/_rels/{name}.rels"), bytes);
    }
    let additions = [(id.as_str(), name.as_str()), (blank_id.as_str(), blank.as_str())]
        .into_iter()
        .map(|(id, name)| {
            format!("<Relationship xmlns=\"{PKG}\" Id=\"{id}\" Type=\"{R}/header\" Target=\"{name}\"/>")
        })
        .collect::<String>();
    package.insert(rels_path.into(), insert_root(&rels, &additions)?.into_bytes());
    let types = xml(package, "[Content_Types].xml")?.to_string();
    let additions = [&name, &blank].into_iter().map(|name| format!("<Override xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\" PartName=\"/word/{name}\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.header+xml\"/>")).collect::<String>();
    package.insert("[Content_Types].xml".into(), insert_root(&types, &additions)?.into_bytes());
    let footer = add_blank_footer(package, suffix)?;
    Ok(AddedHeaders { number: id, blank: blank_id, footer })
}

fn add_blank_footer(package: &mut Package, suffix: u32) -> Result<String> {
    let (name, id) = (format!("erp-blank-footer-{suffix}.xml"), format!("rIdErpBlankFooter{suffix}"));
    package.insert(format!("word/{name}"), format!("<w:ftr xmlns:w=\"{W}\"><w:p/></w:ftr>").into_bytes());
    let path = "word/_rels/document.xml.rels";
    let rels = xml(package, path)?.to_string();
    let relation =
        format!("<Relationship xmlns=\"{PKG}\" Id=\"{id}\" Type=\"{R}/footer\" Target=\"{name}\"/>");
    package.insert(path.into(), insert_root(&rels, &relation)?.into_bytes());
    let types = xml(package, "[Content_Types].xml")?.to_string();
    let addition = format!(
        "<Override xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\" PartName=\"/word/{name}\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.footer+xml\"/>"
    );
    package.insert("[Content_Types].xml".into(), insert_root(&types, &addition)?.into_bytes());
    Ok(id)
}

fn available_header_suffix(package: &Package, relationships: &Document<'_>) -> Result<u32> {
    (1..=4096)
        .find(|i| {
            !package.contains_key(&format!("word/erp-number-{i}.xml"))
                && !package.contains_key(&format!("word/erp-blank-{i}.xml"))
                && !package.contains_key(&format!("word/erp-blank-footer-{i}.xml"))
                && !relationships.descendants().any(|n| {
                    n.attribute("Id") == Some(format!("rIdErpNumber{i}").as_str())
                        || n.attribute("Id") == Some(format!("rIdErpBlank{i}").as_str())
                        || n.attribute("Id") == Some(format!("rIdErpBlankFooter{i}").as_str())
                })
        })
        .ok_or_else(invalid)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(body: &str) -> Vec<u8> {
        pack(Package::from([
            ("_rels/.rels".into(), format!("<Relationships xmlns=\"{PKG}\"><Relationship Id=\"rootDoc\" Type=\"{R}/officeDocument\" Target=\"word/document.xml\"/></Relationships>").into_bytes()),
            ("word/document.xml".into(), format!("<w:document xmlns:w=\"{W}\" xmlns:r=\"{R}\"><w:body>{body}</w:body></w:document>").into_bytes()),
            ("word/_rels/document.xml.rels".into(), format!("<Relationships xmlns=\"{PKG}\"><Relationship Id=\"rId1\" Type=\"{R}/header\" Target=\"header1.xml\"/><Relationship Id=\"rId2\" Type=\"{R}/footer\" Target=\"footer1.xml\"/></Relationships>").into_bytes()),
            ("word/header1.xml".into(), format!("<w:hdr xmlns:w=\"{W}\"><w:p><w:r><w:t>原页眉</w:t></w:r></w:p></w:hdr>").into_bytes()),
            ("word/footer1.xml".into(), format!("<w:ftr xmlns:w=\"{W}\"><w:p/></w:ftr>").into_bytes()),
            ("[Content_Types].xml".into(), b"<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"><Override PartName=\"/word/document.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml\"/></Types>".to_vec()),
            ("word/media/logo.png".into(), vec![1,2,3]),
        ])).unwrap()
    }

    #[test]
    fn preserves_body_media_default_header_footer_and_only_numbers_first_section() {
        let body = "<w:p><w:r><w:t>合同正文</w:t></w:r><w:pPr><w:sectPr><w:headerReference r:id=\"rId1\" w:type=\"default\"/><w:footerReference r:id=\"rId2\" w:type=\"default\"/><w:pgMar w:top=\"1440\"/></w:sectPr></w:pPr></w:p><w:sectPr><w:titlePg/></w:sectPr>";
        let original = fixture(body);
        let output = stamp(&original, "FSY-S-260453").unwrap();
        let before = unpack(&original).unwrap();
        let after = unpack(&output).unwrap();
        assert_eq!(before["word/media/logo.png"], after["word/media/logo.png"]);
        assert_eq!(before["word/header1.xml"], after["word/header1.xml"]);
        assert!(xml(&after, "word/erp-number-1.xml").unwrap().contains("FSY-S-260453"));
        assert!(xml(&after, "word/erp-number-1.xml").unwrap().contains("原页眉"));
        let document = parse(xml(&after, "word/document.xml").unwrap()).unwrap();
        let sections = document.descendants().filter(|n| n.has_tag_name((W, "sectPr"))).collect::<Vec<_>>();
        assert_eq!(reference(sections[0], "headerReference", "first"), Some("rIdErpNumber1"));
        assert_eq!(reference(sections[0], "footerReference", "first"), Some("rId2"));
        assert_eq!(reference(sections[1], "headerReference", "first"), Some("rIdErpBlank1"));
        assert_eq!(reference(sections[1], "footerReference", "first"), Some("rIdErpBlankFooter1"));
        for section in sections {
            let references = section
                .children()
                .filter(|n| n.is_element())
                .map(|n| n.tag_name().name())
                .collect::<Vec<_>>();
            let last_header = references.iter().rposition(|tag| *tag == "headerReference").unwrap();
            let first_footer = references.iter().position(|tag| *tag == "footerReference").unwrap();
            assert!(last_header < first_footer);
        }
        assert!(document.descendants().any(|n| n.text() == Some("合同正文")));
    }

    #[test]
    fn supports_empty_section_and_rejects_bad_packages() {
        let output = stamp(&fixture("<w:p/><w:sectPr/>"), "GYL-S-260016").unwrap();
        let package = unpack(&output).unwrap();
        parse(xml(&package, "word/document.xml").unwrap()).unwrap();
        assert!(stamp(b"not a docx", "FSY-S-260453").is_err());
        assert!(stamp(&fixture("<w:p/>"), "FSY-S-260453").is_err());
        assert!(stamp(&fixture("<w:sectPr/>"), "bad\nnumber").is_err());
    }

    #[test]
    fn retains_original_first_header_footer_inheritance_and_header_images() {
        let body = "<w:p><w:pPr><w:sectPr><w:headerReference r:id=\"rId1\" w:type=\"first\"/><w:footerReference r:id=\"rId2\" w:type=\"first\"/><w:titlePg/></w:sectPr></w:pPr></w:p><w:sectPr><w:titlePg/></w:sectPr>";
        let mut package = unpack(&fixture(body)).unwrap();
        let relationships = format!("<Relationships xmlns=\"{PKG}\"><Relationship Id=\"logo\" Type=\"{R}/image\" Target=\"media/logo.png\"/></Relationships>").into_bytes();
        package.insert("word/_rels/header1.xml.rels".into(), relationships.clone());
        let input = pack(package).unwrap();
        let output = stamp(&input, "GYL-S-260016").unwrap();
        let package = unpack(&output).unwrap();
        assert_eq!(package["word/_rels/erp-number-1.xml.rels"], relationships);
        let document = parse(xml(&package, "word/document.xml").unwrap()).unwrap();
        let sections = document.descendants().filter(|n| n.has_tag_name((W, "sectPr"))).collect::<Vec<_>>();
        assert_eq!(reference(sections[0], "footerReference", "first"), Some("rId2"));
        assert_eq!(reference(sections[1], "footerReference", "first"), Some("rId2"));
        assert_eq!(reference(sections[1], "headerReference", "first"), Some("rId1"));
    }
}
