//! 域 D06 `access_control` 仓储：permission、user_role、data_scope、audit_event。
//!
//! P0 已实现 accounts / roles / audit_logs 仓储（`account_core.rs` / `role.rs` /
//! `audit_log.rs`），本目录只承载四个新增集合（按集合拆分为子模块，经本文件重导出）。
//! 单一集合 CRUD 与乐观锁直接复用 [`Repository`] 基类（base.rs：`update`/`soft_delete` /
//! `restore` 比较 `id + version` 做 CAS，版本不匹配返回
//! [`persistence_core::Error::OptimisticLockingError`]）；子模块只补充域特有查询与
//! 跨集合多步骤写入入口。集合名常量统一从 `extensions::AccessControlExt`
//! 关联常量定义（conventions §4.3），调用方签名不变。
//!
//! `audit_event` 是事实型审计留痕（§4.5.4：不可编辑、不可删除），本目录
//! **不提供**软删除/恢复方法；筛选类型定义在各子模块；相同字段的查询投影直接复用响应 DTO。
use application_core::SortDir;
use mongodb::bson::{Document, doc};

use crate::entity::access_control::paging::{PageParams, TimeSort};

pub mod audit_event;
pub mod data_scope;
pub mod permission;
pub mod person_scope;
pub mod personal_grant;

pub use audit_event::{AuditEventFilter, AuditEventRepositoryExt, AuditEventRow};
pub use data_scope::{DataScopeRepositoryExt, data_scope_subjects_filter};
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
    use mongodb::bson::{Bson, doc};
    use persistence_core::{NoTransaction, QueryFilter};

    use super::{AuditEventFilter, PageParams, PermissionFilter, data_scope_subjects_filter, sort_doc};
    use crate::entity::access_control::{AuditEventResult, DataScopeSubjectType};
    use crate::repository::owned::DataScopeRepository;
    use crate::repository::prelude::*;

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

    /// 批量主体查询保留正常与可能缺失的 ID，由 MongoDB 只返回实际事实。
    #[test]
    fn data_scope_subjects_filter_uses_subject_type_and_id_set() {
        let ids = vec!["role-1".to_string(), "missing-role".to_string()];
        let filter = data_scope_subjects_filter(DataScopeSubjectType::Role, &ids).unwrap();

        assert_eq!(filter.get_i32("schema_version").unwrap(), 2);
        assert_eq!(filter.get_str("subject_type").unwrap(), "role");
        assert_eq!(
            filter.get_document("subject_id").unwrap().get_array("$in").unwrap(),
            &vec![Bson::String("role-1".to_string()), Bson::String("missing-role".to_string())]
        );
    }

    /// 空主体集合必须短路，不得构造可扩大范围的查询。
    #[tokio::test]
    async fn data_scope_subjects_empty_input_does_not_touch_database() {
        assert!(data_scope_subjects_filter(DataScopeSubjectType::Role, &[]).is_none());
        let client = mongodb::Client::with_uri_str("mongodb://127.0.0.1:1").await.unwrap();
        let database = client.database("repository_data_scope_empty_subject_ids");
        let repository = DataScopeRepository::new(&database, "data_scopes");

        let scopes =
            repository.list_by_subjects(DataScopeSubjectType::Role, &[], &mut NoTransaction).await.unwrap();

        assert!(scopes.is_empty());
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
