//! 身份域账号、权限、人员范围、组织及审计事件的索引登记。

use mongodb::bson::{Document, doc};
use mongodb::{Database, IndexModel};
use persistence_core::Result;
use persistence_core::indexes::{named_index, unique_index};

use crate::repository::policy_receipt::POLICY_RECEIPTS;
use crate::repository::portal::PortalIdentityExt;
use crate::repository::{AccessControlExt, CASBIN_RULES};

/// 启动时为供应商门户绑定创建稳定 ID、账号唯一及按供应商查询索引。撤销绑定仍占用账号身份；已有重复数据或建索引失败时返回错误。
pub async fn ensure_portal_indexes(db: &Database) -> Result<()> {
    create_indexes(
        db,
        <Database as PortalIdentityExt>::PORTAL_BINDINGS,
        vec![
            unique_index("uk_supplier_portal_bindings_id", doc! { "id": 1 }),
            unique_index("uk_supplier_portal_bindings_account", doc! { "account_id": 1 }),
            named_index(
                "idx_supplier_portal_bindings_supplier",
                doc! { "supplier_id": 1, "deleted_at": 1, "account_id": 1 },
            ),
        ],
    )
    .await
}

const ACCOUNTS: &str = "accounts";
const ROLES: &str = "roles";
/// `audit_event` 集合名（单一来源：`AccessControlExt` 关联常量）。
const AUDIT_EVENTS: &str = <mongodb::Database as AccessControlExt>::AUDIT_EVENTS;

/// 按启动顺序登记账号、角色和授权相关集合的命名索引。相同定义可重复登记；已有数据违反唯一约束或数据库拒绝建索引时停止。
pub async fn ensure(db: &Database) -> Result<()> {
    ensure_accounts_and_roles(db).await?;
    ensure_authorization(db).await
}

/// 启动装配先登记账号和角色索引，以便后续创建审计日志索引。返回首次持久化错误。
pub async fn ensure_accounts_and_roles(db: &Database) -> Result<()> {
    create_indexes(db, ACCOUNTS, account_indexes()).await?;
    create_indexes(db, ROLES, role_indexes()).await?;
    Ok(())
}

/// 在应用审计日志索引就绪后登记身份的策略、范围、组织、记录授权及审计事件索引。首次建索引失败时停止。
pub async fn ensure_authorization(db: &Database) -> Result<()> {
    create_indexes(db, CASBIN_RULES, casbin_indexes()).await?;

    create_indexes(db, AUDIT_EVENTS, audit_event_indexes()).await?;
    create_indexes(
        db,
        <mongodb::Database as AccessControlExt>::PERSON_QUERY_QUALIFICATIONS,
        person_query_qualification_indexes(),
    )
    .await?;

    ensure_resource_grants(db).await?;
    ensure_person_scopes(db).await?;
    ensure_organizations(db).await?;

    create_indexes(
        db,
        POLICY_RECEIPTS,
        vec![unique_index("uk_authorization_policy_receipts_id", doc! { "id": 1 })],
    )
    .await?;
    Ok(())
}

// 为指定集合应用命名索引定义。重启时可以用相同定义复用已有索引。
async fn create_indexes(db: &Database, collection: &str, indexes: Vec<IndexModel>) -> Result<()> {
    db.collection::<Document>(collection).create_indexes(indexes).await?;
    Ok(())
}

// 定义账号 ID 和登录名全局唯一约束，以及账号列表索引。软删除账号仍保留原登录名，避免恢复身份时发生冲突。
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

// 为人员目录按类别查找资格提供索引，并保证一个账号在同一类别只有一条资格。保留终止记录可阻止初始化再次授予。
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

// 约束角色身份唯一，并为可分配角色列表提供查询索引。
fn role_indexes() -> Vec<IndexModel> {
    vec![
        unique_index("uk_roles_id", doc! { "id": 1 }),
        named_index("idx_roles_active_enabled", doc! { "deleted_at": 1, "disabled": 1 }),
    ]
}

// 为 Casbin 按账号或角色查找和清理策略提供索引；策略行本身使用 MongoDB 的 `_id` 保证身份唯一。
fn casbin_indexes() -> Vec<IndexModel> {
    vec![
        named_index("idx_casbin_ptype_value0", doc! { "sec": 1, "ptype": 1, "values.0": 1 }),
        named_index("idx_casbin_ptype_value1", doc! { "sec": 1, "ptype": 1, "values.1": 1 }),
    ]
}

// 以稳定 ID 防止重复追加审计事件，并加速按操作人、业务对象、请求追踪号和时间查阅。审计历史长期保留，索引不自动清理过期资料。
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

// 启动时建立组织稳定 ID 和成员有效期查询索引；跨记录的写入一致性由全局组织版本事务保证。
async fn ensure_organizations(db: &Database) -> Result<()> {
    use crate::repository::organization::*;
    for collection in [ORG_UNITS, ORG_MEMBERSHIPS, ORG_REVISIONS, ORG_CHANGES] {
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
    .await
}

/// 为人员范围配置登记记录 ID 和账号、资源、动作组合的唯一索引，防止同一动作有多个配置。历史重复数据或建索引失败时返回错误。
pub async fn ensure_person_scopes(db: &Database) -> Result<()> {
    create_indexes(
        db,
        <Database as AccessControlExt>::PERSON_DATA_SCOPES,
        vec![
            unique_index("uk_person_scope_id", doc! { "id": 1 }),
            unique_index("uk_person_scope_action", doc! { "user_id": 1, "resource": 1, "action": 1 }),
        ],
    )
    .await?;
    Ok(())
}

// 登记记录读取授权及其来源、事件、审计所需索引，支持按接收人、目标和来源修订号快速核对有效授权。
async fn ensure_resource_grants(db: &Database) -> Result<()> {
    create_indexes(
        db,
        "resource_grants",
        vec![
            unique_index("uk_resource_grants_id", doc! { "id": 1 }),
            unique_index(
                "uk_resource_grants_source_target",
                doc! { "source_id": 1, "resource": 1, "resource_id": 1, "action": 1 },
            ),
            named_index(
                "idx_resource_grants_recipient",
                doc! { "recipient": 1, "resource": 1, "active": 1, "resource_id": 1 },
            ),
            named_index("idx_resource_grants_source_page", doc! { "source_id": 1, "deleted_at": 1, "id": 1 }),
            named_index("idx_resource_grants_target_read", doc! { "recipient": 1, "resource": 1, "resource_id": 1, "action": 1, "active": 1, "deleted_at": 1 }),
            named_index(
                "idx_resource_grants_source_version",
                doc! { "source_id": 1, "source_version": 1, "resource": 1, "active": 1 },
            ),
        ],
    )
    .await?;
    ensure_resource_grant_sources(db).await?;
    ensure_resource_grant_events(db).await
}

// 为自动和人工授权来源及审计登记唯一身份和查询索引，支持发布后回查及管理员按来源或接收人查看。
async fn ensure_resource_grant_sources(db: &Database) -> Result<()> {
    create_indexes(
        db,
        "resource_grant_sources",
        vec![
            named_index(
                "idx_resource_grant_sources_recent",
                doc! { "key.rule": 1, "updated_at": -1, "id": 1 },
            ),
            unique_index("uk_resource_grant_sources_id", doc! { "id": 1 }),
            unique_index(
                "uk_resource_grant_sources_key",
                doc! { "key.rule": 1, "key.business_type": 1, "key.business_id": 1, "key.recipient": 1 },
            ),
            named_index(
                "idx_resource_grant_sources_recipient",
                doc! { "key.recipient": 1, "deleted_at": 1, "id": 1 },
            ),
        ],
    )
    .await?;
    create_indexes(
        db,
        "resource_grant_audits",
        vec![
            unique_index("uk_resource_grant_audits_id", doc! { "id": 1 }),
            named_index("idx_resource_grant_audits_source", doc! { "source_id": 1, "snapshot.version": 1 }),
        ],
    )
    .await
}

// 为授权事件建立幂等键和领取、恢复、来源检索索引，支持事件追加、后台消费和精确回读。
async fn ensure_resource_grant_events(db: &Database) -> Result<()> {
    create_indexes(
        db,
        "resource_grant_events",
        vec![
            named_index("idx_resource_grant_events_recompute", doc! { "source.rule": 1, "id": 1 }),
            named_index("idx_resource_grant_events_oldest", doc! { "status": 1, "deleted_at": 1, "evidence.occurred_at": 1 }),
            named_index("idx_resource_grant_events_recover", doc! { "status": 1, "lease_until": 1, "id": 1 }),
            named_index("idx_resource_grant_events_source_page", doc! { "source.rule": 1, "source.business_type": 1, "source.business_id": 1, "source.recipient": 1, "id": 1 }),
            unique_index("uk_resource_grant_events_id", doc! { "id": 1 }),
            unique_index("uk_resource_grant_events_dedup_key", doc! { "dedup_key": 1 }),
            named_index(
                "idx_resource_grant_events_claim",
                doc! { "status": 1, "available_at": 1, "lease_until": 1, "id": 1 },
            ),
            named_index(
                "idx_resource_grant_events_typed_source",
                doc! { "source.business_type": 1, "source.business_id": 1, "evidence.fact_version": 1 },
            ),
        ],
    )
    .await
}

#[cfg(test)]
mod tests {
    use mongodb::bson::doc;

    use super::{account_indexes, audit_event_indexes, casbin_indexes};

    // 验证账号 ID 和登录名均全局唯一，软删除也不会释放身份。
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

    // 验证 Casbin 分组规则的主体与角色都可通过相应字段索引查找。
    #[test]
    fn casbin_indexes_cover_both_grouping_policy_positions() {
        let indexes = casbin_indexes();
        assert!(indexes.iter().any(|index| index.keys.contains_key("values.0")));
        assert!(indexes.iter().any(|index| index.keys.contains_key("values.1")));
    }

    // 验证审计可按稳定 ID、操作人、业务对象和请求追踪号定位。
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
