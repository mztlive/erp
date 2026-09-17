use serde::Serialize;

use crate::{Error, Result};

/// 缺省排序字段（创建时间倒序基线）。
pub const DEFAULT_SORT_FIELD: &str = "created_at";
/// 缺省排序方向（降序，新记录优先）。
pub const DEFAULT_SORT_DIR: SortDir = SortDir::Desc;
/// 缺省页码（第一页）。
pub const DEFAULT_PAGE: u64 = 1;
/// 缺省分页大小。
pub const DEFAULT_PAGE_SIZE: u32 = 20;
/// 分页大小上限。
pub const MAX_PAGE_SIZE: u32 = 100;

/// 列表排序方向。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortDir {
    /// 升序。
    Asc,
    /// 降序。
    Desc,
}

/// 按领域白名单归一化列表排序字段与方向。
pub fn normalize_sort(
    sort_by: &Option<String>,
    sort_dir: &Option<String>,
    allowed_fields: &'static [&'static str],
) -> Result<(&'static str, SortDir)> {
    let sort_by = match sort_by.as_deref().map(str::trim).filter(|value| !value.is_empty()) {
        Some(field) => allowed_fields
            .iter()
            .find(|allowed| **allowed == field)
            .copied()
            .ok_or_else(|| Error::ValidationError(format!("不支持的排序字段: {field}")))?,
        None => DEFAULT_SORT_FIELD,
    };
    let sort_dir = match sort_dir.as_deref().map(str::trim).filter(|value| !value.is_empty()) {
        Some("asc") => SortDir::Asc,
        Some("desc") | None => DEFAULT_SORT_DIR,
        Some(other) => return Err(Error::ValidationError(format!("非法排序方向: {other}"))),
    };
    Ok((sort_by, sort_dir))
}

/// 服务列表统一分页响应。
///
/// 所有领域必须返回同一 `items`/`total`/`page`/`page_size` 合同；领域 DTO
/// 只负责列表项与筛选参数，不得重复声明分页信封。
/// 仅需 `items`/`total` 的轻信封改用 `crate::Page`；缺省排序见
/// `DEFAULT_SORT_FIELD` 与 `DEFAULT_SORT_DIR`（创建时间倒序）。
#[derive(Debug, Clone, Serialize)]
pub struct PageView<T> {
    /// 当前页数据。
    pub items: Vec<T>,
    /// 满足筛选条件的总数（非当前页条数）。
    pub total: i64,
    /// 当前页码（1 起）。
    pub page: u64,
    /// 请求的分页大小。
    pub page_size: u32,
}

impl<T> Default for PageView<T> {
    /// 返回首屏空页（`page: 1`，`page_size: 20`）。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回条目为空、总数为零的首页视图。
    ///
    /// # 错误
    /// 无。
    fn default() -> Self {
        Self { items: Vec::new(), total: 0, page: 1, page_size: 20 }
    }
}

/// 校验文本去除首尾空白后非空。
pub fn non_blank(value: &str) -> std::result::Result<(), validator::ValidationError> {
    if value.trim().is_empty() {
        return Err(validator::ValidationError::new("不能为空白"));
    }
    Ok(())
}

/// 归一化可选的文本查询条件。
pub fn normalized_text(value: Option<&str>) -> Option<String> {
    value.map(str::trim).filter(|value| !value.is_empty()).map(str::to_string)
}

/// 返回有效页码；未提供时使用第一页。
pub fn page_or_default(page: Option<u64>) -> u64 {
    page.unwrap_or(DEFAULT_PAGE)
}

/// 返回分页大小；未提供时使用默认大小。
pub fn page_size_or_default(page_size: Option<u32>) -> u32 {
    page_size.unwrap_or(DEFAULT_PAGE_SIZE).clamp(1, MAX_PAGE_SIZE)
}

#[cfg(test)]
mod tests {
    use super::{PageView, page_size_or_default};

    #[test]
    fn page_size_is_bounded_for_non_http_callers() {
        assert_eq!(page_size_or_default(None), 20);
        assert_eq!(page_size_or_default(Some(0)), 1);
        assert_eq!(page_size_or_default(Some(100)), 100);
        assert_eq!(page_size_or_default(Some(u32::MAX)), 100);
    }

    #[test]
    fn page_view_default_is_first_page_size_20() {
        let view: PageView<String> = PageView::default();
        assert!(view.items.is_empty());
        assert_eq!(view.total, 0);
        assert_eq!(view.page, 1);
        assert_eq!(view.page_size, 20);
    }
}
