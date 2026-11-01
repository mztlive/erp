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

/// 已按调用方白名单规范化的分页与排序参数。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageParams {
    /// 页码；入口负责拒绝非法页码，规范化保留显式的零。
    pub page: u64,
    /// 单页条数，规范化后位于 `1..=MAX_PAGE_SIZE`。
    pub page_size: u32,
    /// 调用方白名单中的排序字段，缺省为 `created_at`。
    pub sort_by: &'static str,
    /// 排序方向。
    pub sort_dir: SortDir,
}

impl PageParams {
    /// 返回仓储筛选使用的排序字段。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回排序字段的 owned 副本。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn sort_field(&self) -> String {
        self.sort_by.to_string()
    }

    /// 返回排序方向是否为升序。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 升序返回 `true`，降序返回 `false`。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn sort_ascending(&self) -> bool {
        matches!(self.sort_dir, SortDir::Asc)
    }

    /// 返回采用可选排序字段的仓储筛选参数。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回始终为 `Some` 的排序字段和是否升序。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn sort_selection(&self) -> (Option<String>, bool) {
        (Some(self.sort_field()), self.sort_ascending())
    }
}

/// 规范化分页与排序参数，业务筛选和排序白名单由调用方拥有。
///
/// # 参数
/// * `sort_by` - 可选排序字段，空白按缺省处理。
/// * `sort_dir` - 可选排序方向，空白按降序处理。
/// * `page` - 可选页码，缺省为首页。
/// * `page_size` - 可选单页条数，缺省为 20 并夹紧至 1–100。
/// * `allowed_fields` - 调用方允许的排序字段。
///
/// # 返回
/// 返回可交给仓储的分页排序参数；显式零页码原样保留。
///
/// # 错误
/// 排序字段或方向不合法时返回校验错误；字段错误优先于方向错误。
pub fn normalize_paging(
    sort_by: &Option<String>,
    sort_dir: &Option<String>,
    page: Option<u64>,
    page_size: Option<u32>,
    allowed_fields: &'static [&'static str],
) -> Result<PageParams> {
    let (sort_by, sort_dir) = normalize_sort(sort_by, sort_dir, allowed_fields)?;
    Ok(PageParams {
        page: page_or_default(page),
        page_size: page_size_or_default(page_size),
        sort_by,
        sort_dir,
    })
}

/// 空白判定：去首尾空白后为空即空白，三处查询规范化入口共用。
fn is_blank(value: &str) -> bool {
    value.trim().is_empty()
}

/// 去首尾空白的可选查询条件，三处查询规范化入口共用。
fn trimmed_query(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

/// 按领域白名单归一化排序字段，未提供时回退默认字段。
fn resolve_sort_field(
    sort_by: Option<&str>,
    allowed_fields: &'static [&'static str],
) -> Result<&'static str> {
    match trimmed_query(sort_by) {
        Some(field) => allowed_fields
            .iter()
            .find(|allowed| **allowed == field)
            .copied()
            .ok_or_else(|| Error::ValidationError(format!("不支持的排序字段: {field}"))),
        None => Ok(DEFAULT_SORT_FIELD),
    }
}

/// 归一化排序方向，缺省与显式降序统一为默认方向。
fn resolve_sort_dir(sort_dir: Option<&str>) -> Result<SortDir> {
    match trimmed_query(sort_dir) {
        Some("asc") => Ok(SortDir::Asc),
        Some("desc") | None => Ok(DEFAULT_SORT_DIR),
        Some(other) => Err(Error::ValidationError(format!("非法排序方向: {other}"))),
    }
}

/// 按领域白名单归一化列表排序字段与方向。
///
/// 未提供或空白的排序字段直接使用 `DEFAULT_SORT_FIELD`，不再查白名单。
///
/// # 参数
/// * `sort_by` - 请求中的排序字段。
/// * `sort_dir` - 请求中的排序方向；去掉首尾空白后，空白、未提供或 `desc` 视为 `DEFAULT_SORT_DIR`。
/// * `allowed_fields` - 领域允许的排序字段，与去空白后的 `sort_by` 精确匹配。
///
/// # 返回
/// 返回静态字段名和 `SortDir`。显式 `asc` 为 `SortDir::Asc`。
///
/// # 错误
/// `sort_by` 不在白名单时返回 `Error::ValidationError`。`sort_dir` 不是 `asc` 或 `desc` 时同样返回 `Error::ValidationError`。
pub fn normalize_sort(
    sort_by: &Option<String>,
    sort_dir: &Option<String>,
    allowed_fields: &'static [&'static str],
) -> Result<(&'static str, SortDir)> {
    Ok((resolve_sort_field(sort_by.as_deref(), allowed_fields)?, resolve_sort_dir(sort_dir.as_deref())?))
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
        Self { items: Vec::new(), total: 0, page: DEFAULT_PAGE, page_size: DEFAULT_PAGE_SIZE }
    }
}

/// 校验文本去除首尾空白后非空。
///
/// # 参数
/// * `value` - 待校验文本。
///
/// # 返回
/// 非空白时无返回值。
///
/// # 错误
/// 去首尾空白后为空时返回 `validator::ValidationError`，代码为「不能为空白」。
pub fn non_blank(value: &str) -> std::result::Result<(), validator::ValidationError> {
    if is_blank(value) {
        return Err(validator::ValidationError::new("不能为空白"));
    }
    Ok(())
}

/// 归一化可选的文本查询条件。
///
/// # 参数
/// * `value` - 原始查询文本。
///
/// # 返回
/// 去掉首尾空白后仍非空时返回该文本；未提供或空白时返回 `None`。
///
/// # 错误
/// 不返回错误。
pub fn normalized_text(value: Option<&str>) -> Option<String> {
    trimmed_query(value).map(str::to_string)
}

/// 返回页码；未提供时使用第一页。
///
/// # 参数
/// * `page` - 请求页码。
///
/// # 返回
/// 已提供时原样返回，包括 `0`；未提供时返回 `DEFAULT_PAGE`。
///
/// # 错误
/// 不返回错误。
pub fn page_or_default(page: Option<u64>) -> u64 {
    page.unwrap_or(DEFAULT_PAGE)
}

/// 返回分页大小；未提供时使用默认大小，并夹紧到允许区间。
///
/// # 参数
/// * `page_size` - 请求的分页大小。
///
/// # 返回
/// 未提供时返回 `DEFAULT_PAGE_SIZE`。已提供时夹紧到 `1..=MAX_PAGE_SIZE`。
///
/// # 错误
/// 不返回错误。
pub fn page_size_or_default(page_size: Option<u32>) -> u32 {
    page_size.unwrap_or(DEFAULT_PAGE_SIZE).clamp(1, MAX_PAGE_SIZE)
}

#[cfg(test)]
mod tests {
    use super::{
        DEFAULT_SORT_DIR, DEFAULT_SORT_FIELD, PageView, SortDir, normalize_paging, normalize_sort,
        normalized_text, page_size_or_default,
    };

    #[test]
    fn optional_query_text_trims_unicode_whitespace_and_keeps_absence() {
        assert_eq!(normalized_text(None), None);
        assert_eq!(normalized_text(Some("\u{3000}\t ")), None);
        assert_eq!(normalized_text(Some("\u{3000}商品 🙂 \t")), Some("商品 🙂".into()));
    }

    #[test]
    fn sort_normalization_accepts_allowlisted_field_and_direction() {
        let allowed: &'static [&'static str] = &["created_at", "name"];
        assert_eq!(
            normalize_sort(&Some(" name ".to_string()), &Some("asc".to_string()), allowed).unwrap(),
            ("name", SortDir::Asc)
        );
        assert_eq!(normalize_sort(&None, &None, allowed).unwrap(), (DEFAULT_SORT_FIELD, DEFAULT_SORT_DIR));
        assert_eq!(
            normalize_sort(&Some("  ".to_string()), &Some("desc".to_string()), allowed).unwrap(),
            (DEFAULT_SORT_FIELD, DEFAULT_SORT_DIR)
        );
    }

    #[test]
    fn sort_normalization_rejects_unknown_field_and_direction() {
        let allowed: &'static [&'static str] = &["created_at"];
        assert!(normalize_sort(&Some("name".to_string()), &None, allowed).is_err());
        assert!(normalize_sort(&None, &Some("up".to_string()), allowed).is_err());
    }

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

    #[test]
    fn paging_defaults_and_explicit_bounds_preserve_input_contract() {
        let defaults = normalize_paging(&None, &None, None, None, &["name"]).unwrap();
        assert_eq!((defaults.page, defaults.page_size), (1, 20));
        assert_eq!((defaults.sort_by, defaults.sort_dir), ("created_at", SortDir::Desc));
        let bounded = normalize_paging(
            &Some(" name ".into()),
            &Some(" asc ".into()),
            Some(0),
            Some(u32::MAX),
            &["name"],
        )
        .unwrap();
        assert_eq!((bounded.page, bounded.page_size), (0, 100));
        assert_eq!(bounded.sort_selection(), (Some("name".into()), true));
        assert_eq!(normalize_paging(&None, &None, Some(2), Some(0), &[]).unwrap().page_size, 1);
    }

    #[test]
    fn paging_rejects_sort_errors_before_applying_defaults() {
        let field = normalize_paging(&Some("bad".into()), &Some("up".into()), None, None, &[]).unwrap_err();
        assert_eq!(field.to_string(), "参数验证失败: 不支持的排序字段: bad");
        let direction = normalize_paging(&None, &Some("up".into()), None, None, &[]).unwrap_err();
        assert_eq!(direction.to_string(), "参数验证失败: 非法排序方向: up");
    }
}
