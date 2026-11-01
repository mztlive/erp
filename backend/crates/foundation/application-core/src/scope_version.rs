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
    /// 返回既有跨页查询提示，不包含协议错误码。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回缺失或漂移对应的静态提示。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn detail(self) -> &'static str {
        match self {
            Self::Missing => "请从第一页刷新后继续查询",
            Self::Changed => "数据范围已变化，请从第一页刷新",
        }
    }
}

impl<'a> ScopeVersion<'a> {
    /// 按调用入口的空字符串合同构造版本输入。
    ///
    /// # 参数
    /// * `provided` - 调用方原样回传的快照版本。
    /// * `empty` - 空字符串按已提供或缺失处理。
    ///
    /// # 返回
    /// 返回不执行规范化的版本输入。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn new(provided: Option<&'a str>, empty: EmptyScopeVersion) -> Self {
        Self { provided, empty }
    }

    /// 校验第二页及之后是否携带版本。
    ///
    /// # 参数
    /// * `page` - 请求页码；首页和零页码不额外要求版本。
    ///
    /// # 返回
    /// 首页或存在版本时成功。
    ///
    /// # 错误
    /// 后续页缺失版本时返回 `ScopeVersionError::Missing`。
    pub fn require_for_page(self, page: u64) -> Result<(), ScopeVersionError> {
        if page > 1 && self.value().is_none() {
            return Err(ScopeVersionError::Missing);
        }
        Ok(())
    }

    /// 校验回传版本是否与当前快照一致。
    ///
    /// # 参数
    /// * `current` - 本次读取获得的当前快照版本。
    ///
    /// # 返回
    /// 未提供版本或精确匹配时成功。
    ///
    /// # 错误
    /// 已提供版本不匹配时返回 `ScopeVersionError::Changed`。
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
