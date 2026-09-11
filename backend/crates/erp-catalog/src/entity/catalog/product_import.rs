//! 产品报价表导入的纯领域规则：空白占位、条码、DISPIMG 与截断。

/// 商品名称写入修订时的最大字符数。
pub const IMPORT_NAME_MAX_CHARS: usize = 128;
/// 规格写入修订时的最大字符数。
pub const IMPORT_SPEC_MAX_CHARS: usize = 1024;
/// 条码写入修订时的最大字符数。
pub const IMPORT_BARCODE_MAX_CHARS: usize = 128;
/// 视为未填写的占位文本。
const PLACEHOLDERS: &[&str] = &["-", "—", "－", "无", "N/A", "n/a"];

/// 判断单元格是否为空白或占位符。
///
/// # 参数
/// * `value` - 单元格原文
///
/// # 返回
/// 去空白后为空或属于占位符时返回 `true`。
///
/// # 错误
/// 无。
pub fn is_import_blank(value: &str) -> bool {
    let trimmed = value.trim();
    trimmed.is_empty()
        || PLACEHOLDERS
            .iter()
            .any(|token| trimmed.eq_ignore_ascii_case(token))
}

/// 折叠空白并去掉首尾空白。
///
/// # 参数
/// * `value` - 原文
///
/// # 返回
/// 连续空白（含换行）压缩为单个空格后的文本。
///
/// # 错误
/// 无。
pub fn collapse_import_text(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// 截断到最大字符数，超出时保留省略号。
///
/// # 参数
/// * `value` - 已规范化文本
/// * `max_chars` - 最大字符数，至少为 1
///
/// # 返回
/// 不超过上限的文本。
///
/// # 错误
/// 无。
pub fn truncate_import_text(value: &str, max_chars: usize) -> String {
    let count = value.chars().count();
    if count <= max_chars {
        return value.to_string();
    }
    let keep = max_chars.saturating_sub(1);
    let mut out: String = value.chars().take(keep).collect();
    out.push('…');
    out
}

/// 取第一个有效条码；多行或多个分隔符时只保留第一项。
///
/// # 参数
/// * `value` - 条码单元格
///
/// # 返回
/// 第一个非占位条码；没有有效条码时返回 `None`。
///
/// # 错误
/// 无。
pub fn first_import_barcode(value: &str) -> Option<String> {
    let candidate = value
        .split(['\n', '\r', ',', '，', ';', '；'])
        .map(str::trim)
        .find(|part| !is_import_blank(part))?;
    let collapsed = collapse_import_text(candidate);
    if collapsed.chars().count() > IMPORT_BARCODE_MAX_CHARS {
        return None;
    }
    Some(collapsed)
}

/// 从 `DISPIMG("ID_...",1)` 公式提取图片身份。
///
/// Excel / WPS 可能写成 `=DISPIMG(...)`、`_xlfn.DISPIMG(...)` 或带 `&quot;`。
///
/// # 参数
/// * `value` - 主图或副图单元格
///
/// # 返回
/// 公式中的图片 ID；不是 DISPIMG 公式时返回 `None`。
///
/// # 错误
/// 无。
pub fn dispimg_id(value: &str) -> Option<&str> {
    let trimmed = value.trim();
    let marker = "DISPIMG(";
    let start = trimmed.find(marker)? + marker.len();
    let rest = trimmed.get(start..)?.trim_start();
    let rest = rest.strip_prefix("&quot;").or_else(|| rest.strip_prefix('"'))?;
    let end = rest.find("&quot;").or_else(|| rest.find('"'))?;
    let id = rest.get(..end)?.trim();
    if id.is_empty() {
        None
    } else {
        Some(id)
    }
}

/// 未填写品牌时使用的稳定名称。
///
/// # 返回
/// 返回占位品牌名称。
///
/// # 错误
/// 无。
pub fn unspecified_brand_name() -> &'static str {
    "未填写品牌"
}

/// 解析导入品牌名称；占位符映射为「未填写品牌」。
///
/// # 参数
/// * `value` - 品牌单元格
///
/// # 返回
/// 规范化后的品牌名称。
///
/// # 错误
/// 无。
pub fn import_brand_name(value: &str) -> String {
    if is_import_blank(value) {
        unspecified_brand_name().to_string()
    } else {
        collapse_import_text(value)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        collapse_import_text, dispimg_id, first_import_barcode, import_brand_name, is_import_blank,
        truncate_import_text, unspecified_brand_name,
    };

    #[test]
    fn blank_tokens_and_whitespace_are_empty() {
        assert!(is_import_blank(""));
        assert!(is_import_blank("  -  "));
        assert!(is_import_blank("—"));
        assert!(!is_import_blank("法丽兹"));
    }

    #[test]
    fn first_barcode_keeps_leading_item() {
        assert_eq!(
            first_import_barcode("6979874900058\n6979874900041").as_deref(),
            Some("6979874900058")
        );
        assert_eq!(first_import_barcode("-"), None);
    }

    #[test]
    fn dispimg_extracts_stable_id() {
        assert_eq!(
            dispimg_id(r#"=DISPIMG("ID_DC3C0313483242B2B7BF94A18EC5BDA4",1)"#),
            Some("ID_DC3C0313483242B2B7BF94A18EC5BDA4")
        );
        assert_eq!(
            dispimg_id(r#"=_xlfn.DISPIMG("ID_19A9D2905B534DB1A393AE8AF11F062A",1)"#),
            Some("ID_19A9D2905B534DB1A393AE8AF11F062A")
        );
        assert_eq!(
            dispimg_id(r#"=_xlfn.DISPIMG(&quot;ID_FBA686C3A81D4BF282FEBA3925300F24&quot;,1)"#),
            Some("ID_FBA686C3A81D4BF282FEBA3925300F24")
        );
        assert_eq!(dispimg_id("普通快递"), None);
    }

    #[test]
    fn brand_placeholder_uses_shared_name() {
        assert_eq!(import_brand_name("-"), unspecified_brand_name());
        assert_eq!(import_brand_name(" 莲香楼 "), "莲香楼");
    }

    #[test]
    fn truncate_keeps_char_budget() {
        assert_eq!(collapse_import_text("莲香楼\n凤梨酥"), "莲香楼 凤梨酥");
        let long = "测".repeat(130);
        let truncated = truncate_import_text(&long, 128);
        assert_eq!(truncated.chars().count(), 128);
        assert!(truncated.ends_with('…'));
    }
}
