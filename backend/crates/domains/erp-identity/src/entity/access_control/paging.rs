//! 权限目录和审计列表共用的已验证分页与排序。
use application_core::{SortDir, normalize_sort};

use crate::{Error, Result};

/// 列表查询允许使用的时间排序字段。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TimeSort {
    /// 按创建时间排序，缺省使用此字段。
    #[default]
    CreatedAt,
    /// 按最后更新时间排序。
    UpdatedAt,
}

/// 权限目录和审计列表传给仓储的已校验分页与排序参数。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageParams {
    /// 从 1 开始的页码，用于计算查询偏移量。
    pub(crate) page: u64,
    /// 单页最多读取的条数，范围为 1 至 100。
    pub(crate) page_size: u32,
    /// 仓储用于排序的创建时间或更新时间。
    pub(crate) sort_by: TimeSort,
    /// 时间排序方向，决定较早或较晚的记录优先返回。
    pub(crate) sort_dir: SortDir,
}
impl Default for PageParams {
    // 缺省读取按创建时间降序排列的第 1 页，每页 20 条。
    fn default() -> Self {
        Self { page: 1, page_size: 20, sort_by: TimeSort::CreatedAt, sort_dir: SortDir::Desc }
    }
}
impl PageParams {
    /// 在查询前解析页码、单页条数及时间排序；缺省读取第 1 页 20 条，按创建时间降序。
    /// 页码须大于 0，单页限 1 至 100 条；排序字段和方向裁剪空白后只接受
    /// `created_at` 或 `updated_at`、`asc` 或 `desc`，纯空白走默认值，其余输入报错。
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
    // 确认分页缺省值一致，非法分页与排序被拒绝，裁剪后合法的显式排序可用。
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
