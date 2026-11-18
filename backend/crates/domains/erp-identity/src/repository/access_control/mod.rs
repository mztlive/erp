//! 权限、角色绑定、人员数据范围和审计事件的专用查询。
//! 审计事件只追加，不提供软删除或恢复入口。

use application_core::SortDir;
use mongodb::bson::{Document, doc};

use crate::entity::access_control::paging::{PageParams, TimeSort};

pub mod audit_event;
pub mod permission;
pub mod person_scope;

pub use audit_event::{AuditEventFilter, AuditEventRepositoryExt, AuditEventRow};
pub use permission::{PermissionFilter, PermissionRepositoryExt, UserRoleRepositoryExt};

/// 将已验证时间排序编译为稳定的数据库排序。
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

/// 权限定义列表投影字段。
///
/// # 参数
/// 无。
///
/// # 返回
/// 返回投影条件文档。
///
/// # 错误
/// 不返回错误。
pub(super) fn permission_projection() -> Document {
    doc! {
        "id": 1,
        "resource": 1,
        "action": 1,
        "name": 1,
        "description": 1,
        "system": 1,
        "disabled": 1,
        "version": 1,
        "created_at": 1,
    }
}

/// 审计事件列表投影字段。
///
/// # 参数
/// 无。
///
/// # 返回
/// 返回投影条件文档。
///
/// # 错误
/// 不返回错误。
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

    use super::{AuditEventFilter, PageParams, PermissionFilter, sort_doc};
    use crate::entity::access_control::AuditEventResult;

    #[test]
    fn permission_filter_applies_resource_regex_and_flags() {
        let filter = PermissionFilter {
            resource: Some("sales_order".to_string()),
            disabled: Some(false),
            system: Some(true),
            paging: Default::default(),
        };

        let document = filter.to_doc();
        assert_eq!(document.get_i64("deleted_at").unwrap(), 0);
        let resource = document.get_document("resource").unwrap();
        assert_eq!(resource.get_str("$regex").unwrap(), "sales_order");
        assert!(!document.get_bool("disabled").unwrap());
        assert!(document.get_bool("system").unwrap());
    }

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
