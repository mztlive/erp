//! 权限目录和审计列表共用的已验证分页与排序。
use application_core::{SortDir, normalize_sort};

use crate::{Error, Result};

/// 固定时间排序字段。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TimeSort {
    #[default]
    CreatedAt,
    UpdatedAt,
}

/// 边界校验后在查询和仓储间传递的分页参数。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageParams {
    pub(crate) page: u64,
    pub(crate) page_size: u32,
    pub(crate) sort_by: TimeSort,
    pub(crate) sort_dir: SortDir,
}
impl Default for PageParams {
    // 未指定列表分页时使用第 1 页、每页 20 条，按创建时间倒序。
    fn default() -> Self {
        Self { page: 1, page_size: 20, sort_by: TimeSort::CreatedAt, sort_dir: SortDir::Desc }
    }
}
impl PageParams {
    /// 解析分页与固定时间排序。
    /// 已验证参数；缺省使用首页、20 条、创建时间降序。
    ///
    /// 非法页码、页大小、排序字段或方向拒绝。
    ///
    /// sort_by、sort_dir 为请求排序，page、page_size 为可选分页。
    pub fn parse(
        sort_by: &Option<String>,
        sort_dir: &Option<String>,
        page: Option<u64>,
        page_size: Option<u32>,
    ) -> Result<Self> {
        let page = page.unwrap_or(1);
        let page_size = page_size.unwrap_or(20);
        if page == 0 || !(1..=100).contains(&page_size) {
            return Err(Error::ValidationError("页码必须大于0，分页大小必须在1-100之间".into()));
        }

        let (field, sort_dir) = normalize_sort(sort_by, sort_dir, &["created_at", "updated_at"])?;
        let sort_by = match field {
            "updated_at" => TimeSort::UpdatedAt,
            "created_at" => TimeSort::CreatedAt,
            _ => unreachable!("排序解析器只返回声明的字段"),
        };
        Ok(Self { page, page_size, sort_by, sort_dir })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pagination_rejects_invalid_values_and_preserves_defaults() {
        assert_eq!(PageParams::parse(&None, &None, None, None).unwrap(), PageParams::default());

        for (page, size) in [(0, 20), (1, 0), (1, 101)] {
            assert!(PageParams::parse(&None, &None, Some(page), Some(size)).is_err());
        }
        assert!(PageParams::parse(&Some("actor_id".into()), &None, None, None).is_err());
        assert!(PageParams::parse(&None, &Some("invalid".into()), None, None).is_err());

        let parsed =
            PageParams::parse(&Some(" updated_at ".into()), &Some("asc".into()), Some(2), Some(100)).unwrap();
        assert_eq!(
            (parsed.page, parsed.page_size, parsed.sort_by, parsed.sort_dir),
            (2, 100, TimeSort::UpdatedAt, SortDir::Asc)
        );
    }
}
