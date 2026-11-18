//! 跨页快照版本校验；权限解析和业务错误映射由调用方拥有。

/// 空字符串在调用入口中的既有含义。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmptyScopeVersion {
    /// 空字符串是已提供的版本，须与当前版本精确比较。
    Provided,
    /// 空字符串等同未提供版本。
    Missing,
}

/// 调用方回传的范围快照版本，不执行 trim 或权限解析。
#[derive(Debug, Clone, Copy)]
pub struct ScopeVersion<'a> {
    provided: Option<&'a str>,
    empty: EmptyScopeVersion,
}

/// 跨页快照版本校验失败；领域负责稳定错误码及错误类型转换。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScopeVersionError {
    /// 后续页缺少必要的版本。
    Missing,
    /// 已提供的版本与当前快照不一致。
    Changed,
}

impl ScopeVersionError {
    /// 为领域提供跨页错误的刷新提示，区分版本缺失与范围变化；协议错误码由领域决定。
    pub fn detail(self) -> &'static str {
        match self {
            Self::Missing => "请从第一页刷新后继续查询",
            Self::Changed => "数据范围已变化，请从第一页刷新",
        }
    }
}

impl<'a> ScopeVersion<'a> {
    /// 接收客户端原样回传的快照版本，由入口明确空串算缺失还是已提供；这里不裁剪空白或判断权限。
    pub fn new(provided: Option<&'a str>, empty: EmptyScopeVersion) -> Self {
        Self { provided, empty }
    }

    /// 查询第二页及以后前确认携带版本，缺失时拒绝未经校验的续读；`0` 和 `1` 不增加该要求。
    pub fn require_for_page(self, page: u64) -> Result<(), ScopeVersionError> {
        if page > 1 && self.value().is_none() {
            return Err(ScopeVersionError::Missing);
        }
        Ok(())
    }

    /// 取得本次快照后精确比对已提供的版本，防止交付旧范围的后续页。
    /// 未提供时跳过比对，后续页的版本必填性由 [`Self::require_for_page`] 检查。
    pub fn verify(self, current: &str) -> Result<(), ScopeVersionError> {
        if self.value().is_some_and(|provided| provided != current) {
            return Err(ScopeVersionError::Changed);
        }
        Ok(())
    }

    fn value(self) -> Option<&'a str> {
        match self.empty {
            EmptyScopeVersion::Provided => self.provided,
            EmptyScopeVersion::Missing => self.provided.filter(|value| !value.is_empty()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{EmptyScopeVersion, ScopeVersion, ScopeVersionError};

    #[test]
    fn later_pages_require_a_version_with_the_selected_empty_contract() {
        for empty in [EmptyScopeVersion::Provided, EmptyScopeVersion::Missing] {
            for page in [0, 1] {
                assert_eq!(ScopeVersion::new(None, empty).require_for_page(page), Ok(()));
                assert_eq!(ScopeVersion::new(Some(""), empty).require_for_page(page), Ok(()));
            }

            assert_eq!(ScopeVersion::new(None, empty).require_for_page(2), Err(ScopeVersionError::Missing));
            assert_eq!(ScopeVersion::new(Some("v1"), empty).require_for_page(2), Ok(()));
            assert_eq!(ScopeVersion::new(Some(" "), empty).require_for_page(2), Ok(()));
        }

        assert_eq!(
            ScopeVersion::new(Some(""), EmptyScopeVersion::Missing).require_for_page(2),
            Err(ScopeVersionError::Missing)
        );
        assert_eq!(ScopeVersion::new(Some(""), EmptyScopeVersion::Provided).require_for_page(2), Ok(()));
    }

    #[test]
    fn version_comparison_keeps_missing_empty_and_exact_match_distinct() {
        for empty in [EmptyScopeVersion::Provided, EmptyScopeVersion::Missing] {
            assert_eq!(ScopeVersion::new(None, empty).verify("v1"), Ok(()));
            assert_eq!(ScopeVersion::new(Some("v1"), empty).verify("v1"), Ok(()));
            assert_eq!(ScopeVersion::new(Some("v0"), empty).verify("v1"), Err(ScopeVersionError::Changed));
            assert_eq!(ScopeVersion::new(Some(" v1 "), empty).verify("v1"), Err(ScopeVersionError::Changed));
        }

        assert_eq!(ScopeVersion::new(Some(""), EmptyScopeVersion::Missing).verify("v1"), Ok(()));
        assert_eq!(
            ScopeVersion::new(Some(""), EmptyScopeVersion::Provided).verify("v1"),
            Err(ScopeVersionError::Changed)
        );
        assert_eq!(ScopeVersion::new(Some(""), EmptyScopeVersion::Provided).verify(""), Ok(()));
    }

    #[test]
    fn version_errors_preserve_distinct_refresh_instructions() {
        assert_eq!(ScopeVersionError::Missing.detail(), "请从第一页刷新后继续查询");
        assert_eq!(ScopeVersionError::Changed.detail(), "数据范围已变化，请从第一页刷新");
    }
}
