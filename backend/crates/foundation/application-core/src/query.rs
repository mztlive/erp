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
    /// 为需要自有字符串的仓储筛选器复制已规范化的排序字段。
    pub fn sort_field(&self) -> String {
        self.sort_by.to_string()
    }

    /// 把应用层排序方向换成仓储筛选器使用的布尔值，只有升序为 `true`。
    pub fn sort_ascending(&self) -> bool {
        matches!(self.sort_dir, SortDir::Asc)
    }

    /// 为接收可选排序字段的仓储筛选器组装参数；规范化后总有排序字段，因此始终为 `Some`。
    pub fn sort_selection(&self) -> (Option<String>, bool) {
        (Some(self.sort_field()), self.sort_ascending())
    }
}

/// 执行列表查询前，按领域白名单校验排序并补齐分页缺省值。缺省按创建时间降序查首页，单页默认 20 条，
/// 传入值限制为 1–100；显式零页码保留给入口校验。字段和方向均不合法时先报字段错误。
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

// 供请求字段校验识别仅含空白的值，避免把空格当作有效输入。
fn is_blank(value: &str) -> bool {
    value.trim().is_empty()
}

// 查询词和排序选项共用裁剪规则，避免空白输入触发筛选或报告未知选项。
fn trimmed_query(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

// 为仓储排序取白名单中的静态字段；只校验显式字段，缺省或空白直接用默认字段。
fn resolve_sort_field(
    sort_by: Option<&str>,
    allowed_fields: &'static [&'static str],
) -> Result<&'static str> {
    match trimmed_query(sort_by) {
        Some(field) => allowed_fields
            .iter()
            .find(|allowed| **allowed == field)
            .copied()
            .ok_or_else(|| Error::ValidationError("列表不支持按此项排序，请重新选择排序项".into())),
        None => Ok(DEFAULT_SORT_FIELD),
    }
}

// 把方向文本转成应用层枚举；缺省、空白和 `desc` 取默认降序，其余非 `asc` 输入报错。
fn resolve_sort_dir(sort_dir: Option<&str>) -> Result<SortDir> {
    match trimmed_query(sort_dir) {
        Some("asc") => Ok(SortDir::Asc),
        Some("desc") | None => Ok(DEFAULT_SORT_DIR),
        Some(_) => Err(Error::ValidationError("排序方向无效，请重新选择升序或降序".into())),
    }
}

/// 构造列表查询前解析排序：裁剪字段并与领域白名单精确匹配，缺省或空白直接用 [`DEFAULT_SORT_FIELD`]。
/// 方向仅接受 `asc` 或 `desc`，缺省或空白时降序；未知字段或方向返回校验错误。
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
    // 为无需保留请求分页参数的无结果场景提供空首页：总数为零，每页 20 条。
    fn default() -> Self {
        Self { items: Vec::new(), total: 0, page: DEFAULT_PAGE, page_size: DEFAULT_PAGE_SIZE }
    }
}

/// 在 validator 校验中拒绝纯空白的必填文本；裁剪后为空时返回代码「不能为空白」，不修改输入。
pub fn non_blank(value: &str) -> std::result::Result<(), validator::ValidationError> {
    if is_blank(value) {
        return Err(validator::ValidationError::new("不能为空白"));
    }
    Ok(())
}

/// 组装筛选条件前去掉查询文本两端空白；缺省或全空白时不添加条件。
pub fn normalized_text(value: Option<&str>) -> Option<String> {
    trimmed_query(value).map(str::to_string)
}

/// 补齐列表的缺省首页页码，但不纠正显式传入的 `0`，合法性仍由入口决定。
pub fn page_or_default(page: Option<u64>) -> u64 {
    page.unwrap_or(DEFAULT_PAGE)
}

/// 进入仓储前统一限制单页开销：缺省为 20，显式值裁剪到 `1..=MAX_PAGE_SIZE`。
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
        assert_eq!(field.to_string(), "参数验证失败: 列表不支持按此项排序，请重新选择排序项");
        let direction = normalize_paging(&None, &Some("up".into()), None, None, &[]).unwrap_err();
        assert_eq!(direction.to_string(), "参数验证失败: 排序方向无效，请重新选择升序或降序");
    }
}
