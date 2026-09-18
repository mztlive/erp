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
//! **不提供**软删除/恢复方法；筛选/行类型定义在各子模块，经 `AccessControlExt`
//! 的关联类型对外暴露。

use mongodb::Database;
use mongodb::bson::{Document, doc};
use persistence_core::{Executor, Result, mongo_ops};

use super::extensions::AccessControlExt;
use crate::entity::access_control::{AuditEvent, UserRole};

/// `user_role` 集合名（单一来源：`AccessControlExt` 关联常量）。
pub(super) const USER_ROLES: &str = <mongodb::Database as AccessControlExt>::USER_ROLES;
/// `audit_event` 集合名（单一来源：`AccessControlExt` 关联常量）。
pub(super) const AUDIT_EVENTS: &str = <mongodb::Database as AccessControlExt>::AUDIT_EVENTS;

pub mod audit_event;
pub mod data_scope;
pub mod permission;

pub use audit_event::{AuditEventFilter, AuditEventRepositoryExt, AuditEventRow};
pub use data_scope::{DataScopeFilter, DataScopeRepositoryExt, DataScopeRow, data_scope_subjects_filter};
pub use permission::{PermissionFilter, PermissionRepositoryExt, PermissionRow, UserRoleRepositoryExt};

/// D06 域专用仓储：跨集合、多步骤且必须位于事务内的聚合写入。
///
/// 单一集合 CRUD 使用 [`Repository`] 基类；本类型只承载依赖事务的
/// 跨集合原子写入入口，由 `AccessControlExt::access_control()` 访问。
pub struct AccessControlRepository<'a> {
    db: &'a Database,
}

impl<'a> AccessControlRepository<'a> {
    /// 创建域专用仓储。
    ///
    /// # 参数
    /// * `db` - 目标 MongoDB 数据库
    ///
    /// # 返回
    /// 返回仓储实例。
    pub fn new(db: &'a Database) -> Self {
        Self { db }
    }

    /// 分配用户角色并追加审计事件（跨集合多步骤写入）。
    ///
    /// 依次写入 `user_roles` 与 `audit_events`，保证「授权绑定 + 审计留痕」
    /// 原子可见（§4.5.4 安全审计与变更留痕）。**必须收到事务执行器**：本方法
    /// 不构成原子边界，传入 `NoTransaction` 时两笔写入各自自动提交，审计失败
    /// 会留下没有审计的绑定；Service 必须通过
    /// `persistence_core::Transactional::with_transaction` 传入事务会话。
    ///
    /// # 参数
    /// * `binding` - 待写入的用户角色绑定
    /// * `event` - 待追加的审计事件
    /// * `executor` - 数据访问执行器，必须位于事务中
    ///
    /// # 错误
    /// 当唯一索引冲突（透出 [`persistence_core::Error::DuplicateKey`]，由 Service 映射
    /// 为冲突语义）或 MongoDB 写入失败时返回错误。
    pub async fn assign_user_role_with_audit(
        &self,
        binding: &UserRole,
        event: &AuditEvent,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        mongo_ops::insert_one(&self.db.collection::<UserRole>(USER_ROLES), binding, executor).await?;
        mongo_ops::insert_one(&self.db.collection::<AuditEvent>(AUDIT_EVENTS), event, executor).await?;
        Ok(())
    }
}

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
pub(super) fn sort_doc(sort_by: Option<&str>, sort_ascending: bool) -> Document {
    let direction = if sort_ascending { 1 } else { -1 };
    let field = match sort_by {
        Some("updated_at") => "updated_at",
        _ => "created_at",
    };
    doc! { field: direction, "id": direction }
}

/// 权限定义列表投影字段。
///
/// # 返回
/// 返回投影条件文档。
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

/// 数据范围列表投影字段。
///
/// # 返回
/// 返回投影条件文档。
pub(super) fn data_scope_projection() -> Document {
    doc! {
        "id": 1,
        "subject_type": 1,
        "subject_id": 1,
        "scope_type": 1,
        "scope_targets": 1,
        "schema_version": 1, "resource": 1, "actions": 1, "target_dimension": 1,
        "target_mode": 1, "include_descendants": 1, "enabled": 1,
        "version": 1,
        "created_at": 1,
    }
}

/// 审计事件列表投影字段。
///
/// # 返回
/// 返回投影条件文档。
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

    use super::{AuditEventFilter, DataScopeFilter, PermissionFilter, data_scope_subjects_filter, sort_doc};
    use crate::entity::access_control::{AuditEventResult, DataScopeSubjectType, DataScopeType};
    use crate::repository::owned::DataScopeRepository;
    use crate::repository::prelude::*;

    #[test]
    fn permission_filter_applies_resource_regex_and_flags() {
        let filter = PermissionFilter {
            resource: Some("sales_order".to_string()),
            disabled: Some(false),
            system: Some(true),
            page: 1,
            page_size: 20,
            sort_by: None,
            sort_ascending: false,
        };

        let document = filter.to_doc();
        assert_eq!(document.get_i64("deleted_at").unwrap(), 0);
        let resource = document.get_document("resource").unwrap();
        assert_eq!(resource.get_str("$regex").unwrap(), "sales_order");
        assert!(!document.get_bool("disabled").unwrap());
        assert!(document.get_bool("system").unwrap());
    }

    #[test]
    fn data_scope_filter_applies_subject_and_scope_type() {
        let filter = DataScopeFilter {
            subject_type: Some(DataScopeSubjectType::Role),
            subject_id: Some("role-sales".to_string()),
            scope_type: Some(DataScopeType::Team),
            resource: Some("sales_order".to_string()),
            action: Some("list".to_string()),
            page: 1,
            page_size: 20,
            sort_by: None,
            sort_ascending: false,
        };

        let document = filter.to_doc();
        assert_eq!(document.get_str("subject_type").unwrap(), "role");
        assert_eq!(document.get_str("subject_id").unwrap(), "role-sales");
        assert_eq!(document.get_str("scope_type").unwrap(), "team");
        assert_eq!(document.get_str("resource").unwrap(), "sales_order");
        assert_eq!(document.get_str("actions").unwrap(), "list");
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
            page: 1,
            page_size: 20,
            sort_by: None,
            sort_ascending: false,
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
    fn sort_doc_defaults_to_created_at_and_whitelists_fields() {
        assert_eq!(sort_doc(None, false), doc! { "created_at": -1, "id": -1 });
        assert_eq!(sort_doc(Some("updated_at"), true), doc! { "updated_at": 1, "id": 1 });
        assert_eq!(
            sort_doc(Some("actor_id"), false),
            doc! { "created_at": -1, "id": -1 },
            "白名单外字段回落默认排序"
        );
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
            page: 1,
            page_size: 20,
            sort_by: None,
            sort_ascending: false,
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
