//! 域 D02 `document_registry` 仓储：business_document、document_relation、document_participant、workflow_action。
//!
//! 单一集合 CRUD 与乐观锁直接复用 [`Repository`] 基类（base.rs：
//! `update`/`soft_delete`/`restore` 比较 `id + version` 做 CAS，版本不匹配返回
//! [`persistence_core::Error::OptimisticLockingError`]）；本模块只补充域特有查询与
//! 跨集合多步骤写入入口。集合名常量统一从 `extensions::DocumentRegistryExt`
//! 关联常量导入（conventions §4.3）。
//!
//! 筛选/行类型定义在本模块，经 `DocumentRegistryExt` 的关联类型对外暴露。

use mongodb::bson::{Document, doc};

mod business_document;
mod document_participant;
mod document_relation;
mod workflow_action;

pub use business_document::{
    ApprovalBindingLookup, BusinessDocumentFilter, BusinessDocumentRepositoryExt, BusinessDocumentRow,
};
pub use document_participant::DocumentParticipantRepositoryExt;
pub use document_relation::DocumentRelationRepositoryExt;
pub use workflow_action::{WorkflowActionFilter, WorkflowActionRepositoryExt, WorkflowActionRow};

/// 构建排序文档（排序字段白名单化，禁止透传任意字段名）。
///
/// 仅允许 `created_at` / `updated_at`；未知字段回落默认 `created_at`。
///
/// # 参数
/// * `sort_by` - 排序字段；`None` 或白名单外字段时默认 `created_at`
/// * `sort_ascending` - 升序为 `true`，降序为 `false`
///
/// # 返回
/// 返回排序条件文档。
///
/// # 错误
/// 无。
fn sort_doc(sort_by: Option<&str>, sort_ascending: bool) -> Document {
    let direction = if sort_ascending { 1 } else { -1 };
    let field = match sort_by {
        Some("updated_at") => "updated_at",
        _ => "created_at",
    };
    doc! { field: direction }
}

#[cfg(test)]
mod tests {
    use erp_core::common::time::Instant;
    use mongodb::bson::doc;
    use persistence_core::QueryFilter;

    use super::business_document::{assign_document_no_pipeline, same_id_registration};
    use super::{BusinessDocumentFilter, WorkflowActionFilter, sort_doc};
    use crate::entity::document_registry::{BusinessDocumentId, DocumentType, WorkflowActionType};
    use crate::repository::bpm::{
        AssignDocumentNoOutcome, assign_document_no_filter, classify_assign_document_no_miss,
    };

    #[test]
    fn business_document_filter_default_uses_first_page() {
        let filter = BusinessDocumentFilter::default();
        assert_eq!(filter.page, 1);
        assert_eq!(filter.page_size, 20);
    }

    #[test]
    fn business_document_filter_applies_type_and_no_regex() {
        let filter = BusinessDocumentFilter {
            document_type: Some(DocumentType::SalesOrder),
            document_no: Some("so-001".to_string()),
            page: 1,
            page_size: 20,
            sort_by: None,
            sort_ascending: false,
        };

        let document = filter.to_doc();
        assert_eq!(document.get_i64("deleted_at").unwrap(), 0);
        assert_eq!(document.get_str("document_type").unwrap(), "sales_order");
        let no = document.get_document("document_no").unwrap();
        assert_eq!(no.get_str("$regex").unwrap(), r"so\-001");
        assert_eq!(no.get_str("$options").unwrap(), "i");
    }

    #[test]
    fn workflow_action_filter_applies_document_and_action_type() {
        let filter = WorkflowActionFilter {
            document_id: Some(BusinessDocumentId::new("order-1")),
            actor_id: None,
            action_type: Some(WorkflowActionType::Approve),
            page: 1,
            page_size: 20,
            sort_by: None,
            sort_ascending: false,
        };

        let document = filter.to_doc();
        assert_eq!(document.get_str("document_id").unwrap(), "order-1");
        assert_eq!(document.get_str("action_type").unwrap(), "approve");
    }

    #[test]
    fn sort_doc_defaults_to_created_at_and_whitelists_fields() {
        assert_eq!(sort_doc(None, false), doc! { "created_at": -1 });
        assert_eq!(sort_doc(Some("created_at"), true), doc! { "created_at": 1 });
        assert_eq!(sort_doc(Some("updated_at"), false), doc! { "updated_at": -1 });
        assert_eq!(
            sort_doc(Some("document_no"), false),
            doc! { "created_at": -1 },
            "白名单外字段回落默认排序"
        );
    }

    #[test]
    fn empty_document_register_same_id_is_idempotent_reread() {
        assert!(same_id_registration(Some(&"bd-1".to_string()), "bd-1", String::as_str));
        assert!(!same_id_registration(Some(&"bd-2".to_string()), "bd-1", String::as_str));
        assert!(!same_id_registration::<String>(None, "bd-1", String::as_str));
    }

    #[test]
    fn assign_document_no_cas_allows_empty_drafts_and_rejects_overwrite() {
        let filter = assign_document_no_filter("bd-1", 2).unwrap();
        assert_eq!(filter.get_str("id").unwrap(), "bd-1");
        assert_eq!(filter.get_i64("version").unwrap(), 2);
        let alternatives = filter.get_array("$or").unwrap();
        assert_eq!(
            alternatives,
            &vec![
                mongodb::bson::Bson::Document(doc! { "document_no": "" }),
                mongodb::bson::Bson::Document(doc! { "document_no": mongodb::bson::Bson::Null }),
            ]
        );

        let pipeline = assign_document_no_pipeline("SO-1", Instant::from_unix_secs(99));
        let set = pipeline[0].get_document("$set").unwrap();
        assert_eq!(set.get_str("document_no").unwrap(), "SO-1");
        assert_eq!(set.get_i64("document_no_assigned_at").unwrap(), 99);
        assert!(matches!(
            classify_assign_document_no_miss(
                Some((1_u64, "SO-1".to_string())),
                1,
                "SO-1",
                |row| row.0,
                |row| row.1.as_str()
            ),
            AssignDocumentNoOutcome::SamePayload(_)
        ));
        assert!(matches!(
            classify_assign_document_no_miss(
                Some((1_u64, "SO-2".to_string())),
                1,
                "SO-1",
                |row| row.0,
                |row| row.1.as_str()
            ),
            AssignDocumentNoOutcome::NumberConflict(_)
        ));
        assert!(matches!(
            classify_assign_document_no_miss(
                Some((2_u64, String::new())),
                1,
                "SO-1",
                |row| row.0,
                |row| row.1.as_str()
            ),
            AssignDocumentNoOutcome::VersionConflict(_)
        ));
        assert!(matches!(
            classify_assign_document_no_miss(
                Some((1_u64, String::new())),
                1,
                "SO-1",
                |row| row.0,
                |row| row.1.as_str()
            ),
            AssignDocumentNoOutcome::VersionConflict(_)
        ));
    }
}
