//! D15 仓储目录内部共享工具：排序白名单与批量查询过滤构建。

use mongodb::bson::{doc, Bson, Document};

/// `purchase_order` 列表允许的排序字段白名单。
pub(super) const PURCHASE_ORDER_SORT_FIELDS: &[&str] = &["created_at", "purchase_no", "status"];
/// `purchase_order_submission` 列表允许的排序字段白名单。
pub(super) const SUBMISSION_SORT_FIELDS: &[&str] = &["created_at", "submission_no", "status"];

/// 构建白名单校验后的排序文档。
///
/// 排序字段必须落在白名单内，未知字段一律回退 `created_at`（§2.3 禁止透传
/// 任意字段名）；`None` 默认 `created_at` 降序。
///
/// # 参数
/// * `sort_by` - 排序字段
/// * `whitelist` - 允许的排序字段集合
/// * `sort_ascending` - 升序为 `true`，降序为 `false`
///
/// # 返回
/// 返回排序条件文档。
pub(super) fn sort_doc(sort_by: Option<&str>, whitelist: &[&str], sort_ascending: bool) -> Document {
    let field = sort_by
        .filter(|field| whitelist.contains(field))
        .unwrap_or("created_at");
    let direction = if sort_ascending { 1 } else { -1 };
    doc! { field: direction }
}

/// 构建 `$in` 批量查询过滤（批量取回，禁止 N+1）。
///
/// # 参数
/// * `field` - 匹配字段名
/// * `values` - 待匹配的 ID 字符串集合
///
/// # 返回
/// 返回批量查询条件文档。
pub(super) fn in_filter(field: &str, values: impl IntoIterator<Item = String>) -> Document {
    let values: Vec<Bson> = values.into_iter().map(Bson::String).collect();
    doc! { field: { "$in": values } }
}

#[cfg(test)]
mod tests {
    use mongodb::bson::doc;

    use super::{in_filter, sort_doc, PURCHASE_ORDER_SORT_FIELDS};

    #[test]
    fn sort_doc_respects_whitelist_and_defaults_to_created_at() {
        assert_eq!(
            sort_doc(None, PURCHASE_ORDER_SORT_FIELDS, false),
            doc! { "created_at": -1 }
        );
        assert_eq!(
            sort_doc(Some("purchase_no"), PURCHASE_ORDER_SORT_FIELDS, true),
            doc! { "purchase_no": 1 }
        );
        assert_eq!(
            sort_doc(Some("status"), PURCHASE_ORDER_SORT_FIELDS, false),
            doc! { "status": -1 }
        );
    }

    #[test]
    fn sort_doc_rejects_unknown_field_with_fallback() {
        assert_eq!(
            sort_doc(Some("arbitrary_field"), PURCHASE_ORDER_SORT_FIELDS, true),
            doc! { "created_at": 1 },
            "未知排序字段必须回退 created_at"
        );
    }

    #[test]
    fn in_filter_builds_bson_string_list() {
        let filter = in_filter("purchase_order_id", ["po-1".to_string(), "po-2".to_string()]);
        assert_eq!(filter, doc! { "purchase_order_id": { "$in": ["po-1", "po-2"] } });
    }
}
