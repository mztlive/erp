//! Crate 内复用的纯查询装配 helpers：深分页守卫、去重与范围错误。
//!
//! 只收敛本 crate 内已重复多次的纯逻辑（分页守卫文案、保序/排序去重、
//! 范围变化错误），不改变任何过滤、排序、总数与回退语义；事实权限
//! 仍由拥有领域执行，本模块不接触数据库。

use std::collections::HashSet;
use std::hash::Hash;

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
    if page > 1 && scope_version.is_none_or(str::is_empty) {
        return Err(data_scope_changed("请从第一页刷新后继续查询"));
    }
    Ok(())
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
        if trimmed.is_empty() || !seen.insert(trimmed.to_string()) {
            continue;
        }
        unique.push(trimmed.to_string());
    }
    unique
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
    fn trimmed_ids_skip_blank_and_dedup() {
        assert_eq!(
            dedup_trimmed_nonempty([" buyer-1 ", "buyer-1", "   ", "buyer-2"]),
            vec!["buyer-1".to_string(), "buyer-2".to_string()]
        );
        assert!(dedup_trimmed_nonempty(["   ", ""]).is_empty());
    }
}
