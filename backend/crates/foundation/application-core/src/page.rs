use serde::Serialize;

/// 应用服务对外返回的分页结果。
///
/// 该类型属于 Service/API 合同，不暴露仓储层的分页结果类型。
/// 需要页码与页大小的完整信封改用 `query::PageView`；本类型仅承载 `items`/`total`。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Page<T> {
    pub items: Vec<T>,
    pub total: i64,
}

impl<T> Page<T> {
    /// 取数和计数完成后，用当前页与符合条件的总条数构造轻量应用响应，不包含页码或页大小。
    pub fn new(items: Vec<T>, total: i64) -> Self {
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
