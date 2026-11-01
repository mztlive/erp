use std::collections::HashMap;

use roxmltree::{Document as XmlDocument, ParsingOptions};

use super::{Archive, invalid};
use crate::Result;

const RELATIONSHIP_NS: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const STRICT_RELATIONSHIP_NS: &str = "http://purl.oclc.org/ooxml/officeDocument/relationships";

/// 按禁用 DTD 的配置解析 XML。
/// # 参数
/// `xml` 为 UTF-8 原文。
/// # 返回
/// 已完成实体解码的 XML 文档。
/// # 错误
/// XML 损坏或声明 DTD 时拒绝。
pub(crate) fn document(xml: &str) -> Result<XmlDocument<'_>> {
    XmlDocument::parse_with_options(xml, ParsingOptions { allow_dtd: false, ..Default::default() })
        .map_err(|_| invalid("工作簿 XML 内容损坏或包含不允许的实体声明"))
}

/// 读取工作簿日期系统。
/// # 参数
/// `xml` 为 workbook.xml 原文。
/// # 返回
/// 启用 1904 日期系统时为真。
/// # 错误
/// XML 损坏时返回校验错误。
pub(crate) fn date1904(xml: &str) -> Result<bool> {
    let doc = document(xml)?;
    Ok(doc
        .descendants()
        .find(|node| node.is_element() && node.tag_name().name() == "workbookPr")
        .and_then(|node| node.attribute("date1904"))
        .is_some_and(|value| matches!(value, "1" | "true")))
}

/// 读取关系并将目标规范化为包内安全路径。
/// # 参数
/// `xml` 为关系 XML，`source` 为关系所属文件。
/// # 返回
/// 关系标识到包内路径的映射。
/// # 错误
/// 外部目标、越界路径、重复或缺失标识及 XML 损坏时拒绝。
pub(crate) fn relationships(xml: &str, source: &str) -> Result<HashMap<String, String>> {
    let doc = document(xml)?;
    let mut paths = HashMap::new();
    for node in doc.descendants().filter(|node| node.is_element() && node.tag_name().name() == "Relationship")
    {
        if node.attribute("TargetMode") == Some("External") {
            return Err(invalid("工作簿不得使用外部引用"));
        }
        let id = node.attribute("Id").ok_or_else(|| invalid("工作簿关系标识缺失"))?;
        let target = node.attribute("Target").ok_or_else(|| invalid("工作簿关系路径缺失"))?;
        let path = resolve_target(source, target)?;
        if paths.insert(id.to_string(), path).is_some() {
            return Err(invalid("工作簿关系标识重复"));
        }
    }
    Ok(paths)
}

/// 读取工作表名称和路径；模板白名单由调用方执行。
/// # 参数
/// `archive` 为已校验的工作簿。
/// # 返回
/// 按工作簿顺序排列的工作表名称和包内路径。
/// # 错误
/// 元数据缺失、重复表名、关系无效或 XML 损坏时拒绝。
pub(crate) fn sheet_paths(archive: &mut Archive<'_>) -> Result<Vec<(String, String)>> {
    let workbook = archive.xml("xl/workbook.xml")?;
    let rels = archive.xml("xl/_rels/workbook.xml.rels")?;
    let paths = relationships(&rels, "xl/workbook.xml")?;
    let doc = document(&workbook)?;
    let mut sheets = Vec::new();
    for node in doc.descendants().filter(|node| node.is_element() && node.tag_name().name() == "sheet") {
        let name = node.attribute("name").ok_or_else(|| invalid("工作表名称无效"))?;
        let id = node
            .attribute((RELATIONSHIP_NS, "id"))
            .or_else(|| node.attribute((STRICT_RELATIONSHIP_NS, "id")))
            .ok_or_else(|| invalid("工作表引用无效"))?;
        let path = paths.get(id).ok_or_else(|| invalid("工作表路径缺失"))?;
        if sheets.iter().any(|(existing, _)| existing == name) {
            return Err(invalid("工作表名称重复"));
        }
        sheets.push((name.to_string(), path.clone()));
    }
    Ok(sheets)
}

fn resolve_target(source: &str, target: &str) -> Result<String> {
    if target.is_empty() || target.contains(['\\', ':', '\0', '?', '#']) {
        return Err(invalid("工作簿关系路径不安全"));
    }
    let mut parts = if target.starts_with('/') || target.starts_with("xl/") {
        Vec::new()
    } else {
        source.rsplit_once('/').map_or(Vec::new(), |(parent, _)| parent.split('/').collect())
    };
    for part in target.trim_start_matches('/').split('/') {
        match part {
            "" | "." => {},
            ".." => {
                parts.pop().ok_or_else(|| invalid("工作簿关系路径越界"))?;
            },
            _ => parts.push(part),
        }
    }
    if parts.is_empty() {
        return Err(invalid("工作簿关系路径缺失"));
    }
    Ok(parts.join("/"))
}
