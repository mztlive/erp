//! Support HTTP/application DTOs reused by handlers.
//!
//! `PageParams`（归一化分页）与分页 helpers（`SortDir`/`PageView`/
//! `non_blank`/`normalize_sort`）是三 DTO 共享的唯一来源；各列表 DTO 只保留
//! 各自的排序白名单常量与筛选字段（契约字段与校验语义不变）。

pub mod bulk_job;
pub mod file_asset;
pub mod source_registry;

/// 契约目标形状的分页响应（三 DTO 共用）。
pub use application_core::PageView;
/// 排序方向（三 DTO 共用，透出 `application_core::SortDir`）。
pub use application_core::SortDir;
/// 校验文本去除首尾空白后非空（三 DTO 共用）。
pub use application_core::non_blank;
/// 校验排序参数并返回归一化排序字段与方向（三 DTO 共用）。
pub(crate) use application_core::normalize_sort;
use application_core::{page_or_default, page_size_or_default};
use serde::{Deserialize, Serialize};

use crate::error::Result;

/// 归一化后的分页查询 DTO（Service → Repository 共用）。
///
/// 各列表 `normalized()` 共用本结构携带分页与排序结果；筛选字段保留在
/// 各自的 `*ListQuery` 中。`sort_by` 的 `&'static str` 保证来源只可能是
/// 调用方传入的白名单。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageParams {
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数（已 clamp 到 1–100）。
    pub page_size: u32,
    /// 排序字段（已过白名单校验）。
    pub sort_by: &'static str,
    /// 排序方向。
    pub sort_dir: SortDir,
}

impl PageParams {
    /// 用原始分页输入与已归一化的排序结果组装分页参数。
    ///
    /// # 参数
    /// * `page` - 原始页码输入；`None` 时取默认值
    /// * `page_size` - 原始单页条数输入；`None` 时取默认值
    /// * `sort_by` - 已过白名单校验的排序字段
    /// * `sort_dir` - 已归一化的排序方向
    ///
    /// # 返回
    /// 返回归一化后的分页参数。
    pub fn normalized(
        page: Option<u64>,
        page_size: Option<u32>,
        sort_by: &'static str,
        sort_dir: SortDir,
    ) -> Self {
        Self { page: page_or_default(page), page_size: page_size_or_default(page_size), sort_by, sort_dir }
    }

    /// 用原始分页与排序输入直接组装分页参数（各列表 `normalized()` 共用）。
    ///
    /// 把“排序白名单校验 + 分页默认值”两步收敛为一次调用；排序语义与
    /// 逐个调用 `normalize_sort` 后再 `normalized` 完全一致。
    ///
    /// # 参数
    /// * `page` - 原始页码输入；`None` 时取默认值
    /// * `page_size` - 原始单页条数输入；`None` 时取默认值
    /// * `sort_by` - 原始排序字段输入（过白名单校验）
    /// * `sort_dir` - 原始排序方向输入（`asc`/`desc`）
    /// * `allowed` - 该列表允许的排序字段白名单
    ///
    /// # 返回
    /// 返回归一化后的分页参数。
    ///
    /// # 错误
    /// 排序字段不在白名单或排序方向非法时返回 `ValidationError`。
    pub fn resolve(
        page: Option<u64>,
        page_size: Option<u32>,
        sort_by: &Option<String>,
        sort_dir: &Option<String>,
        allowed: &'static [&'static str],
    ) -> Result<Self> {
        let (sort_by, sort_dir) = normalize_sort(sort_by, sort_dir, allowed)?;
        Ok(Self::normalized(page, page_size, sort_by, sort_dir))
    }

    /// 将归一化分页映射为仓储 `Filter` 的分页与排序字段。
    ///
    /// 各列表 Service 用本方法填充 `page`/`page_size`/`sort_by`/
    /// `sort_ascending`，只保留各自的筛选字段组装。
    ///
    /// # 返回
    /// 返回 `(page, page_size, sort_by, sort_ascending)`。
    pub fn into_filter_parts(self) -> (u64, u32, Option<String>, bool) {
        let ascending = matches!(self.sort_dir, SortDir::Asc);
        (self.page, self.page_size, Some(self.sort_by.to_string()), ascending)
    }
}

/// 各列表 DTO 共用的扁平分页请求字段（`page`/`page_size`/`sort_by`/`sort_dir`）。
///
/// 契约保持扁平传递；本结构只收敛字段形状与校验属性，各 DTO 以
/// `#[serde(flatten)]` 内嵌（序列化形状不变）。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FlatPageParams {
    /// 页码（1 起）。
    pub page: Option<u64>,
    /// 单页条数（1–100）。
    pub page_size: Option<u32>,
    /// 排序字段（白名单由各 DTO 的 `normalized()` 校验）。
    pub sort_by: Option<String>,
    /// 排序方向（`asc`/`desc`）。
    pub sort_dir: Option<String>,
}
