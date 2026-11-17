//! Crate 内复用的纯查询与导出装配：深分页守卫、去重、范围错误与 CSV 编码。
//!
//! 只收敛本 crate 内复用的纯逻辑（分页守卫文案、保序/排序去重、
//! 范围变化错误、CSV 编码），不改变任何过滤、排序、总数与回退语义；
//! 事实权限仍由拥有领域执行，本模块不接触数据库。

use std::collections::HashSet;
use std::hash::Hash;

use application_core::{EmptyScopeVersion, ScopeVersion};
use rust_decimal::Decimal;

use crate::{Error, Result};

/// 深分页必须携带范围版本，禁止不同授权页拼接。
///
/// # 参数
/// * `page` - 请求页码（已规整为从 1 起）
/// * `scope_version` - 调用方回传的范围版本
///
/// # 返回
/// 首页或携带非空版本时成功。
///
/// # 错误
/// * `ConflictError` - 后续页缺失范围版本时拒绝，要求从第一页刷新
pub(crate) fn ensure_deep_page(page: u64, scope_version: Option<&str>) -> Result<()> {
    ScopeVersion::new(scope_version, EmptyScopeVersion::Missing)
        .require_for_page(page)
        .map_err(|error| data_scope_changed(error.detail()))
}

/// 以统一前缀构造范围变化冲突错误。
///
/// # 参数
/// * `detail` - `DATA_SCOPE_CHANGED：` 之后的具体说明
///
/// # 返回
/// 返回带统一前缀的 `ConflictError`。
///
/// # 错误
/// 无。
pub(crate) fn data_scope_changed(detail: &str) -> Error {
    Error::ConflictError(format!("DATA_SCOPE_CHANGED：{detail}"))
}

/// 保序去重，保留首次出现顺序。
///
/// # 参数
/// * `values` - 待去重的迭代器
///
/// # 返回
/// 返回按首次出现顺序去重后的集合。
///
/// # 错误
/// 无。
pub(crate) fn dedup_ordered<T>(values: impl IntoIterator<Item = T>) -> Vec<T>
where
    T: Hash + Eq + Clone,
{
    let mut seen = HashSet::new();
    let mut unique = Vec::new();
    for value in values {
        if seen.insert(value.clone()) {
            unique.push(value);
        }
    }
    unique
}

/// 排序去重，供跨域批量 `$in` 候选稳定生成。
///
/// # 参数
/// * `values` - 待去重的迭代器
///
/// # 返回
/// 返回按升序稳定排序的唯一集合。
///
/// # 错误
/// 无。
pub(crate) fn dedup_sorted<T>(values: impl IntoIterator<Item = T>) -> Vec<T>
where
    T: Ord,
{
    let mut values = values.into_iter().collect::<Vec<_>>();
    values.sort();
    values.dedup();
    values
}

/// 去空白并保序去重账号 ID。
///
/// # 参数
/// * `values` - 原始账号 ID 引用迭代器，允许空白与重复
///
/// # 返回
/// 返回去空白、去重后按首次出现顺序排列的账号 ID。
///
/// # 错误
/// 无。
pub(crate) fn dedup_trimmed_nonempty<'a>(values: impl IntoIterator<Item = &'a str>) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut unique = Vec::new();
    for value in values {
        let trimmed = value.trim();
        if trimmed.is_empty() || !seen.insert(trimmed) {
            continue;
        }
        unique.push(trimmed.to_string());
    }
    unique
}

/// 编码报表 CSV 行，危险文本加单引号，合法十进制数值保留原文。
///
/// # 参数
/// * `values` - 按报表列顺序排列的原始单元格。
///
/// # 返回
/// 返回各单元格加双引号、内部引号转义后以逗号连接的行。
///
/// # 错误
/// 不返回错误。
pub(crate) fn csv_record(values: &[&str]) -> String {
    values.iter().map(|value| csv_cell(value)).collect::<Vec<_>>().join(",")
}

fn csv_cell(value: &str) -> String {
    let trimmed = value.trim_start();
    let numeric = value.parse::<Decimal>().is_ok();
    let dangerous =
        !numeric && (trimmed.starts_with(['=', '+', '-', '@']) || value.starts_with(['\t', '\r', '\n']));
    let prefix = if dangerous { "'" } else { "" };
    format!("\"{prefix}{}\"", value.replace('"', "\"\""))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deep_page_guard_accepts_first_page_and_versioned_later_pages() {
        assert!(ensure_deep_page(1, None).is_ok());
        assert!(ensure_deep_page(2, Some("v1")).is_ok());
        assert!(ensure_deep_page(2, None).is_err());
        assert!(ensure_deep_page(2, Some("")).is_err());
        assert!(
            matches!(ensure_deep_page(2, None).unwrap_err(), Error::ConflictError(message) if message == "DATA_SCOPE_CHANGED：请从第一页刷新后继续查询")
        );
    }

    #[test]
    fn scope_changed_error_keeps_unified_prefix() {
        let error = data_scope_changed("请从第一页刷新后继续查询");
        assert!(
            matches!(error, Error::ConflictError(message) if message == "DATA_SCOPE_CHANGED：请从第一页刷新后继续查询")
        );
    }

    #[test]
    fn ordered_dedup_keeps_first_seen_order() {
        assert_eq!(
            dedup_ordered(["sup-1".to_string(), "sup-2".to_string(), "sup-1".to_string()]),
            vec!["sup-1".to_string(), "sup-2".to_string()]
        );
        assert!(dedup_ordered(Vec::<String>::new()).is_empty());
    }

    #[test]
    fn sorted_dedup_is_stable_for_batch_queries() {
        assert_eq!(
            dedup_sorted(["b".to_string(), "a".to_string(), "b".to_string()]),
            vec!["a".to_string(), "b".to_string()]
        );
    }

    #[test]
    fn csv_record_preserves_numbers_and_escapes_structure() {
        assert_eq!(
            csv_record(&["客户", "-12.50", "0", "12.50", "a,b\nc", "say \"hi\""]),
            "\"客户\",\"-12.50\",\"0\",\"12.50\",\"a,b\nc\",\"say \"\"hi\"\"\""
        );
        assert_eq!(csv_record(&[]), "");
        assert_eq!(csv_record(&["", "   "]), "\"\",\"   \"");
    }

    #[test]
    fn csv_record_neutralizes_formulas_and_non_numeric_prefixes() {
        for (value, expected) in [
            (" =HYPERLINK(\"x\")", "\"' =HYPERLINK(\"\"x\"\")\""),
            ("\u{2003}=1+1", "\"'\u{2003}=1+1\""),
            ("+SUM(A1:A2)", "\"'+SUM(A1:A2)\""),
            ("-not-a-number", "\"'-not-a-number\""),
            ("@SUM(A1)", "\"'@SUM(A1)\""),
            ("\ttext", "\"'\ttext\""),
            ("\rtext", "\"'\rtext\""),
            ("\ntext", "\"'\ntext\""),
        ] {
            assert_eq!(csv_record(&[value]), expected);
        }
        assert_eq!(csv_record(&["-12.50%", "-", "+"]), "\"'-12.50%\",\"'-\",\"'+\"");
    }

    #[test]
    fn trimmed_ids_skip_blank_and_dedup() {
        assert_eq!(
            dedup_trimmed_nonempty([" buyer-1 ", "buyer-1", "   ", "buyer-2"]),
            vec!["buyer-1".to_string(), "buyer-2".to_string()]
        );
        assert!(dedup_trimmed_nonempty(["   ", ""]).is_empty());
    }
}
