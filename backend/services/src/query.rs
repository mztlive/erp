/// 归一化可选的文本查询条件。
pub(crate) fn normalized_text(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

/// 返回有效页码；未提供时使用第一页。
pub(crate) fn page_or_default(page: Option<u64>) -> u64 {
    page.unwrap_or(1)
}

/// 返回分页大小；未提供时使用默认大小。
pub(crate) fn page_size_or_default(page_size: Option<u32>) -> u32 {
    page_size.unwrap_or(20).clamp(1, 100)
}

#[cfg(test)]
mod tests {
    use super::page_size_or_default;

    #[test]
    fn page_size_is_bounded_for_non_http_callers() {
        assert_eq!(page_size_or_default(None), 20);
        assert_eq!(page_size_or_default(Some(0)), 1);
        assert_eq!(page_size_or_default(Some(100)), 100);
        assert_eq!(page_size_or_default(Some(u32::MAX)), 100);
    }
}
