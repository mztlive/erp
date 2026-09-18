//! 五类列表筛选共用的查询前缀与分页口径（查询语义与分页口径不变）。
//!
//! 各筛选类型的 `to_doc` 只组装业务条件，未删除过滤统一由 [`active_filter`]
//! 提供；`page_and_size` 的 `(page, page_size)` 元组口径统一由 [`page_and_size`]
//! 提供，避免在五个筛选文件中重复字面量。

/// 未删除过滤文档（五类列表 `to_doc` 共用前缀；查询语义不变）。
///
/// # 返回
/// 返回仅含未删除条件的查询文档；调用方追加业务条件。
pub(crate) fn active_filter() -> mongodb::bson::Document {
    mongodb::bson::doc! { "deleted_at": entity_core::NOT_DELETED_TIMESTAMP_BSON }
}

/// 通用分页口径（五类列表 `page_and_size` 共用；分页语义不变）。
///
/// # 参数
/// * `page` - 页码（1 起）
/// * `page_size` - 单页条数
///
/// # 返回
/// 返回 `(page, page_size)` 元组。
pub(crate) fn page_and_size(page: u64, page_size: u32) -> (u64, u64) {
    (page, u64::from(page_size))
}

#[cfg(test)]
mod tests {
    use super::active_filter;

    /// 共用前缀只含未删除条件；业务条件由各筛选 `to_doc` 追加。
    #[test]
    fn active_filter_holds_only_not_deleted_condition() {
        let filter = active_filter();
        assert_eq!(filter.len(), 1);
        assert!(filter.contains_key("deleted_at"));
    }
}
