//! 人员数据范围和审计事件的专用查询。
//! 审计事件只追加，不提供软删除或恢复入口。

use application_core::SortDir;
use mongodb::bson::{Document, doc};

use crate::entity::access_control::paging::{PageParams, TimeSort};

pub mod audit_event;
pub mod person_scope;

pub use audit_event::{AuditEventFilter, AuditEventRepositoryExt, AuditEventRow};

// 将已校验的时间排序转为 MongoDB 排序，并用同向 ID 打破并列。
fn sort_doc(paging: &PageParams) -> Document {
    let direction = match paging.sort_dir {
        SortDir::Asc => 1,
        SortDir::Desc => -1,
    };
    let field = match paging.sort_by {
        TimeSort::CreatedAt => "created_at",
        TimeSort::UpdatedAt => "updated_at",
    };
    doc! { field: direction, "id": direction }
}

/// 限定审计列表只读取安全投影字段，不加载完整事件。
pub(super) fn audit_event_projection() -> Document {
    doc! {
        "id": 1,
        "actor_id": 1,
        "actor_label": 1,
        "actor_role": 1,
        "action_type": 1,
        "object_type": 1,
        "object_id": 1,
        "object_label": 1,
        "request_id": 1,
        "result": 1,
        "changed_field_names": 1,
        "source_ip": 1,
        "created_at": 1,
    }
}

#[cfg(test)]
mod tests {
    use mongodb::bson::doc;
    use persistence_core::QueryFilter;

    use super::{AuditEventFilter, PageParams, sort_doc};
    use crate::entity::access_control::AuditEventResult;

    // 验证审计操作者与动作的正则字符被转义，且对象和结果按精确值筛选。
    #[test]
    fn audit_event_filter_applies_regex_and_object_fields() {
        let filter = AuditEventFilter {
            q: None,
            keyword_actions: None,
            event_id: None,
            trace_id: None,
            created_from: None,
            created_before: None,
            actor_id: Some("user-1".to_string()),
            action_type: Some("sales_order.approve".to_string()),
            object_type: Some("sales_order".to_string()),
            object_id: Some("SO-1".to_string()),
            result: Some(AuditEventResult::Denied),
            paging: Default::default(),
        };

        let document = filter.to_doc();
        let actor = document.get_document("actor_id").unwrap();
        assert_eq!(actor.get_str("$regex").unwrap(), r"user\-1");
        let action = document.get_document("action_type").unwrap();
        assert_eq!(action.get_str("$regex").unwrap(), r"sales_order\.approve");
        assert_eq!(document.get_str("object_type").unwrap(), "sales_order");
        assert_eq!(document.get_str("object_id").unwrap(), "SO-1");
        assert_eq!(document.get_str("result").unwrap(), "DENIED");
    }

    // 验证默认及显式时间排序均以同向 ID 作稳定次序。
    #[test]
    fn sorting_compiles_validated_fields_and_stable_id_order() {
        assert_eq!(sort_doc(&PageParams::default()), doc! { "created_at": -1, "id": -1 });
        let paging = PageParams::parse(&Some("updated_at".into()), &Some("asc".into()), None, None).unwrap();
        assert_eq!(sort_doc(&paging), doc! { "updated_at": 1, "id": 1 });
    }
}

#[cfg(test)]
mod keyword_regression_tests {
    use persistence_core::QueryFilter;

    use super::*;
    // 验证关键词和动作标签不会覆盖事件 ID、追踪号及时间条件。
    #[test]
    fn keyword_preserves_structural_scope() {
        let mut filter = AuditEventFilter {
            q: None,
            keyword_actions: None,
            event_id: None,
            trace_id: None,
            created_from: None,
            created_before: None,
            actor_id: None,
            action_type: None,
            object_type: None,
            object_id: None,
            result: None,
            paging: Default::default(),
        };

        filter.q = Some("张.[1]".into());
        filter.trace_id = Some("trace".into());
        filter.event_id = Some("event".into());
        filter.keyword_actions = Some("customer.create".into());
        filter.created_from = Some(10);
        filter.created_before = Some(20);

        let query = filter.to_doc();
        assert_eq!(query.get_str("id").unwrap(), "event");
        assert_eq!(query.get_document("created_at").unwrap().get_i64("$lt").unwrap(), 20);
        assert!(query.contains_key("$or"));
        assert!(query.contains_key("$and"));

        let text = format!("{query:?}");
        assert!(text.contains("actor_label"));
        assert!(text.contains("customer.create"));
    }
}
