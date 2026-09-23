//! 域 D06 `access_control` 的索引声明：账号、角色、审计日志、Casbin 规则。
//!
//! P0 从 `indexes.rs` 整体迁入既有索引（accounts/roles/audit_logs/casbin），
//! 职责不变；P2 追加 permission、user_role、data_scope、audit_event 的索引声明。

use mongodb::bson::{Document, doc};
use mongodb::options::IndexOptions;
use mongodb::{Database, IndexModel};
use persistence_core::Result;

use crate::repository::{AccessControlExt, CASBIN_RULES};

const ACCOUNTS: &str = "accounts";
const ROLES: &str = "roles";
/// `permission` 集合名（单一来源：`AccessControlExt` 关联常量）。
const PERMISSIONS: &str = <mongodb::Database as AccessControlExt>::PERMISSIONS;
/// `user_role` 集合名（单一来源：`AccessControlExt` 关联常量）。
const USER_ROLES: &str = <mongodb::Database as AccessControlExt>::USER_ROLES;
/// `data_scope` 集合名（单一来源：`AccessControlExt` 关联常量）。
const DATA_SCOPES: &str = <mongodb::Database as AccessControlExt>::DATA_SCOPES;
/// `audit_event` 集合名（单一来源：`AccessControlExt` 关联常量）。
const AUDIT_EVENTS: &str = <mongodb::Database as AccessControlExt>::AUDIT_EVENTS;

/// 创建本域集合的幂等命名索引。
///
/// 账号在软删除后仍保留原身份，因此 `account` 使用全局唯一索引，避免账号复用
/// 破坏后续恢复语义。P2 追加部分落地 W19 配置化权限（数据模型 §5.1 / §4.6）：
/// - `permission`：`(resource, action)` 全局唯一（定义目录身份），停用标记查询索引；
/// - `user_role`：同一用户同一角色同时最多一条**未撤权**绑定（部分唯一索引，
///   理由与回滚方式见 `user_role_indexes` 注释）；按用户分列展示索引；
/// - `data_scope`：同一主体同一范围类型唯一；范围类型查询索引；
/// - `audit_event`：事实型审计留痕（§4.5.4），**不设 TTL**（长期保留），
///   按操作者、业务对象、请求追踪号与时间倒序查询索引。
///
/// # 参数
/// * `db` - 目标 MongoDB 数据库
///
/// # 错误
/// 当已有数据违反唯一约束或 MongoDB 无法创建索引时返回错误。
pub async fn ensure(db: &Database) -> Result<()> {
    ensure_accounts_and_roles(db).await?;
    ensure_authorization(db).await
}

/// 创建账号与角色索引，供启动根在审计日志索引之前调用。
///
/// # 错误
/// 第一个索引创建失败时返回原持久化错误，不继续创建后续集合索引。
pub async fn ensure_accounts_and_roles(db: &Database) -> Result<()> {
    create_indexes(db, ACCOUNTS, account_indexes()).await?;
    create_indexes(db, ROLES, role_indexes()).await?;
    Ok(())
}

/// 创建身份授权与审计事件索引，供启动根在审计日志索引之后调用。
///
/// # 错误
/// 按原集合顺序返回第一个持久化错误。
pub async fn ensure_authorization(db: &Database) -> Result<()> {
    create_indexes(db, CASBIN_RULES, casbin_indexes()).await?;
    create_indexes(db, PERMISSIONS, permission_indexes()).await?;
    create_indexes(db, USER_ROLES, user_role_indexes()).await?;
    create_indexes(db, DATA_SCOPES, data_scope_indexes()).await?;
    remove_obsolete_scope_index(db).await?;
    create_indexes(db, AUDIT_EVENTS, audit_event_indexes()).await?;
    create_indexes(
        db,
        <mongodb::Database as AccessControlExt>::PERSON_QUERY_QUALIFICATIONS,
        person_query_qualification_indexes(),
    )
    .await?;
    ensure_organizations(db).await?;
    Ok(())
}

/// 为单个集合创建一组幂等命名索引。
async fn create_indexes(db: &Database, collection: &str, indexes: Vec<IndexModel>) -> Result<()> {
    db.collection::<Document>(collection).create_indexes(indexes).await?;
    Ok(())
}

/// 返回账号集合的身份约束和列表查询索引。
fn account_indexes() -> Vec<IndexModel> {
    vec![
        unique_index("uk_accounts_id", doc! { "id": 1 }),
        unique_index("uk_accounts_account", doc! { "account": 1 }),
        named_index(
            "idx_accounts_kind_active_created",
            doc! { "kind": 1, "deleted_at": 1, "created_at": -1 },
        ),
        named_index("idx_accounts_kind_active_name", doc! { "kind": 1, "deleted_at": 1, "name": 1, "id": 1 }),
    ]
}

/// 返回人员查询资格的身份唯一约束和按类别读取索引。
///
/// 同一账号同一类别只有一条资格。终止后保留该行，初始化不能再插入第二条。
fn person_query_qualification_indexes() -> Vec<IndexModel> {
    vec![
        unique_index("uk_person_query_qualifications_id", doc! { "id": 1 }),
        unique_index(
            "uk_person_query_qualifications_account_category",
            doc! { "account_id": 1, "category": 1 },
        ),
        named_index(
            "idx_person_query_qualifications_category_status",
            doc! { "category": 1, "status": 1, "account_id": 1 },
        ),
    ]
}

/// 返回角色身份约束和可分配角色查询索引。
fn role_indexes() -> Vec<IndexModel> {
    vec![
        unique_index("uk_roles_id", doc! { "id": 1 }),
        named_index("idx_roles_active_enabled", doc! { "deleted_at": 1, "disabled": 1 }),
    ]
}

/// 返回 Casbin 按主体或角色清理 policy 所需的查询索引。
///
/// 规则身份由 MongoDB 内建的 `_id` 唯一索引保证。
fn casbin_indexes() -> Vec<IndexModel> {
    vec![
        named_index("idx_casbin_ptype_value0", doc! { "sec": 1, "ptype": 1, "values.0": 1 }),
        named_index("idx_casbin_ptype_value1", doc! { "sec": 1, "ptype": 1, "values.1": 1 }),
    ]
}

/// 返回 `permission` 定义目录的身份约束与停用查询索引。
///
/// `(resource, action)` 是权限定义身份（W19 权限目录按 `resource:action` 唯一
/// 维护），身份类字段全局唯一（与 accounts 处理一致）：软删除后仍保留定义，
/// 避免复用破坏审计与授权绑定语义。
fn permission_indexes() -> Vec<IndexModel> {
    vec![
        unique_index("uk_permissions_resource_action", doc! { "resource": 1, "action": 1 }),
        named_index("idx_permissions_disabled", doc! { "disabled": 1 }),
    ]
}

/// 返回 `user_role` 的未撤权绑定唯一约束与按用户查询索引。
///
/// 「同一用户同一角色同时仅一条有效绑定」（W19 §5.1）使用**部分唯一索引**
/// 落地：撤权是审计动作，历史撤权记录必须累积保留（按当前/未来/已过期分开
/// 只读展示），因此唯一性只约束 `revoked_at` 为空的未撤权绑定，不能做成
/// 全局唯一。到期未撤权的绑定再次授权时先走撤权命令，保证撤权审计链完整。
///
/// 部分过滤表达式用 `{ revoked_at: null }`（空值相等）：实体 `revoked_at` 为
/// `None` 时按 BSON null 落库，空值相等同时命中「缺省字段」与「显式 null」，
/// 撤权写入时间戳后不再命中、唯一约束随之释放；MongoDB 8 不再支持
/// `$exists: false` 形态的部分索引表达式，故不用取反写法。
///
/// 回滚方式：若业务规则收紧为「同一用户同一角色全生命周期只有一条绑定」，
/// 删除本部分唯一索引，改为全局唯一索引 `uk_user_roles_binding`。
fn user_role_indexes() -> Vec<IndexModel> {
    vec![
        IndexModel::builder()
            .keys(doc! { "user_id": 1, "role_id": 1 })
            .options(
                IndexOptions::builder()
                    .name("uk_user_roles_active".to_string())
                    .unique(true)
                    .partial_filter_expression(doc! { "revoked_at": null })
                    .build(),
            )
            .build(),
        named_index("idx_user_roles_user_effective", doc! { "user_id": 1, "effective_from": -1 }),
    ]
}

/// 返回 `data_scope` 的主体范围唯一约束与范围类型查询索引。
///
/// `(subject_type, subject_id, scope_type)` 是数据范围身份：同一主体同一范围
/// 类型一条记录承载全部范围目标（W19 §5.1）；身份类字段全局唯一（与 accounts
/// 处理一致），软删除后不释放身份。
fn data_scope_indexes() -> Vec<IndexModel> {
    vec![
        unique_index("uk_data_scopes_id", doc! { "id": 1 }),
        named_index("idx_data_scopes_scope_type", doc! { "scope_type": 1 }),
        named_index(
            "idx_data_scopes_subject_resource_action",
            doc! { "subject_type": 1, "subject_id": 1, "resource": 1, "actions": 1, "enabled": 1, "deleted_at": 1 },
        ),
    ]
}

/// 返回 `audit_event` 的追加式审计查询索引。
///
/// 审计事件是事实型留痕（§4.5.4，不可编辑、不可删除、**不设 TTL**，长期保留），
/// 无自然业务唯一键，用 `id` 唯一索引防止重复身份静默写入；按操作者、业务
/// 对象、请求追踪号与时间倒序覆盖安全审计查询路径。
fn audit_event_indexes() -> Vec<IndexModel> {
    vec![
        unique_index("uk_audit_events_id", doc! { "id": 1 }),
        named_index("idx_audit_events_actor_created", doc! { "actor_id": 1, "created_at": -1 }),
        named_index(
            "idx_audit_events_object_created",
            doc! { "object_type": 1, "object_id": 1, "created_at": -1 },
        ),
        named_index("idx_audit_events_created", doc! { "created_at": -1 }),
        named_index("idx_audit_events_request_id", doc! { "request_id": 1 }),
    ]
}

/// 构建命名普通索引。
fn named_index(name: impl Into<String>, keys: Document) -> IndexModel {
    IndexModel::builder().keys(keys).options(IndexOptions::builder().name(name.into()).build()).build()
}

/// 构建命名唯一索引。
fn unique_index(name: impl Into<String>, keys: Document) -> IndexModel {
    IndexModel::builder()
        .keys(keys)
        .options(IndexOptions::builder().name(name.into()).unique(true).build())
        .build()
}

/// 组织实体身份、成员有效期与管理范围的索引；跨行约束由全局版本事务防止写偏差。
async fn ensure_organizations(db: &Database) -> Result<()> {
    use crate::repository::organization::*;
    for collection in [ORG_UNITS, ORG_MEMBERSHIPS, ORG_MANAGEMENT, ORG_REVISIONS, ORG_CHANGES] {
        create_indexes(db, collection, vec![unique_index(format!("uk_{collection}_id"), doc! { "id": 1 })])
            .await?;
    }
    create_indexes(
        db,
        ORG_UNITS,
        vec![named_index("idx_org_parent_status", doc! { "parent_id": 1, "enabled": 1 })],
    )
    .await?;
    create_indexes(
        db,
        ORG_MEMBERSHIPS,
        vec![
            named_index(
                "idx_membership_user_validity",
                doc! { "user_id": 1, "valid_from": 1, "valid_to": 1 },
            ),
            named_index(
                "idx_membership_org_validity",
                doc! { "org_unit_id": 1, "valid_from": 1, "valid_to": 1 },
            ),
        ],
    )
    .await?;
    create_indexes(
        db,
        ORG_MANAGEMENT,
        vec![named_index(
            "idx_management_user_role_validity",
            doc! { "user_id": 1, "role_id": 1, "valid_from": 1, "valid_to": 1 },
        )],
    )
    .await
}

/// 移除阻止同主体按资源配置多条规则的旧索引；不回填或解释旧范围数据。
async fn remove_obsolete_scope_index(db: &Database) -> Result<()> {
    let collection = db.collection::<Document>(DATA_SCOPES);
    if collection.list_index_names().await?.iter().any(|name| name == "uk_data_scopes_subject_scope") {
        collection.drop_index("uk_data_scopes_subject_scope").await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use mongodb::bson::doc;

    use super::{
        account_indexes, audit_event_indexes, casbin_indexes, data_scope_indexes, permission_indexes,
        user_role_indexes,
    };

    #[test]
    fn account_identity_indexes_are_globally_unique() {
        let indexes = account_indexes();

        for name in ["uk_accounts_id", "uk_accounts_account"] {
            let index = indexes
                .iter()
                .find(|index| {
                    index.options.as_ref().and_then(|options| options.name.as_deref()) == Some(name)
                })
                .unwrap();
            let options = index.options.as_ref().unwrap();
            assert_eq!(options.unique, Some(true));
            assert!(options.partial_filter_expression.is_none());
        }
    }

    #[test]
    fn casbin_indexes_cover_both_grouping_policy_positions() {
        let indexes = casbin_indexes();

        assert!(indexes.iter().any(|index| index.keys.contains_key("values.0")));
        assert!(indexes.iter().any(|index| index.keys.contains_key("values.1")));
    }

    #[test]
    fn permission_identity_is_globally_unique() {
        let indexes = permission_indexes();

        let identity = indexes
            .iter()
            .find(|index| {
                index.options.as_ref().and_then(|options| options.name.as_deref())
                    == Some("uk_permissions_resource_action")
            })
            .unwrap();
        assert_eq!(identity.keys, doc! { "resource": 1, "action": 1 });
        assert_eq!(identity.options.as_ref().unwrap().unique, Some(true));
        assert!(identity.options.as_ref().unwrap().partial_filter_expression.is_none());
        assert!(indexes.iter().any(|index| index.keys == doc! { "disabled": 1 }));
    }

    #[test]
    fn user_role_uniqueness_is_partial_over_unrevoked_bindings() {
        let indexes = user_role_indexes();

        let active = indexes
            .iter()
            .find(|index| {
                index.options.as_ref().and_then(|options| options.name.as_deref())
                    == Some("uk_user_roles_active")
            })
            .unwrap();
        assert_eq!(active.keys, doc! { "user_id": 1, "role_id": 1 });
        assert_eq!(active.options.as_ref().unwrap().unique, Some(true));
        assert_eq!(
            active.options.as_ref().unwrap().partial_filter_expression,
            Some(doc! { "revoked_at": null })
        );
        assert!(indexes.iter().any(|index| { index.keys == doc! { "user_id": 1, "effective_from": -1 } }));
    }

    #[test]
    fn data_scope_identity_is_unique_and_resource_rules_can_coexist() {
        let indexes = data_scope_indexes();

        assert!(indexes.iter().any(|index| {
            index.options.as_ref().and_then(|options| options.name.as_deref()) == Some("uk_data_scopes_id")
                && index.options.as_ref().and_then(|options| options.unique) == Some(true)
        }));
        assert!(indexes.iter().any(|index| index.keys == doc! { "scope_type": 1 }));
    }

    #[test]
    fn audit_event_indexes_cover_actor_object_and_request_trace() {
        let indexes = audit_event_indexes();

        assert!(indexes.iter().any(|index| {
            index.options.as_ref().and_then(|options| options.name.as_deref()) == Some("uk_audit_events_id")
                && index.options.as_ref().and_then(|options| options.unique) == Some(true)
        }));
        assert!(indexes.iter().any(|index| index.keys == doc! { "actor_id": 1, "created_at": -1 }));
        assert!(
            indexes
                .iter()
                .any(|index| { index.keys == doc! { "object_type": 1, "object_id": 1, "created_at": -1 } })
        );
        assert!(indexes.iter().any(|index| index.keys == doc! { "request_id": 1 }));
    }
}
