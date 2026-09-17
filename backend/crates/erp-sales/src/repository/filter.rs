//! 本域列表仓储共用的过滤拼装 helper。
//!
//! `sales_order` 与 `sales_selection` 的 `to_doc` 同样需要未删除过滤与稳定
//! 排序文档；两侧只声明自有字段条件，拼装形态收敛到本模块。字面量正则统一
//! 使用 `persistence_core::insert_literal_regex_filter`（两侧不再各自定义转义）。
//! `sales_review` 的排序文档无 `id` 平局决胜（历史行为），不经过本模块。

use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use mongodb::bson::{Document, doc};

/// 未删除行条件（`deleted_at` 哨兵等值）。
///
/// # 返回
/// 返回单条件查询文档。
pub(crate) fn undeleted_condition() -> Document {
    doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON }
}

/// 向 `$and` 数组追加未删除条件。
///
/// # 参数
/// * `and` - 待追加的 `$and` 条件数组
pub(crate) fn push_undeleted(and: &mut Vec<Document>) {
    and.push(undeleted_condition());
}

/// 构建排序文档（默认 `created_at` 倒序，`id` 同向平局决胜保证稳定分页）。
///
/// # 参数
/// * `sort_by` - 排序字段；`None` 时默认 `created_at`
/// * `sort_ascending` - 升序为 `true`，降序为 `false`
///
/// # 返回
/// 返回排序条件文档。
pub(crate) fn sort_doc(sort_by: Option<&str>, sort_ascending: bool) -> Document {
    let direction = if sort_ascending { 1 } else { -1 };
    doc! { sort_by.unwrap_or("created_at"): direction, "id": direction }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn undeleted_condition_uses_sentinel() {
        assert_eq!(undeleted_condition(), doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON });
        let mut and = Vec::new();
        push_undeleted(&mut and);
        assert_eq!(and, vec![doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON }]);
    }

    #[test]
    fn sort_doc_defaults_to_created_at_descending_with_id_tiebreak() {
        assert_eq!(sort_doc(None, false), doc! { "created_at": -1, "id": -1 });
        assert_eq!(sort_doc(Some("order_no"), true), doc! { "order_no": 1, "id": 1 });
    }
}
