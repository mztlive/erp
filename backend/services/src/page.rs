use serde::Serialize;

/// 应用服务对外返回的分页结果。
///
/// 该类型属于 Service/API 合同，不暴露仓储层的分页结果类型。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Page<T> {
    pub items: Vec<T>,
    pub total: i64,
}

impl<T> Page<T> {
    /// 使用数据项和总数构造分页结果。
    ///
    /// # 参数
    /// * `items` - 当前页数据
    /// * `total` - 满足查询条件的数据总数
    ///
    /// # 返回值
    /// 返回分页结果。
    pub(crate) fn new(items: Vec<T>, total: i64) -> Self {
        Self { items, total }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::Page;

    #[test]
    fn serializes_existing_items_and_total_contract() {
        let page = Page::new(vec!["item"], 1);

        assert_eq!(
            serde_json::to_value(page).unwrap(),
            json!({
                "items": ["item"],
                "total": 1
            })
        );
    }
}
