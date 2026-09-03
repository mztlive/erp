//! 域 D09 `supplier` 的索引声明：supplier_account、supplier_commercial_profile_revision、
//! supplier_capability(+_revision)、supplier_qualification(+_revision)、
//! supplier_qualification_capability、supplier_rating_revision（数据模型 §6.2）。
//!
//! 集合名常量取 `SupplierExt` 关联常量（唯一权威来源，conventions §4.3）：
//! `indexes/` 与 `repository/` 均为冻结声明下的私有子树，模块路径无法互相引用，
//! 关联常量随 trait 公开可达，两侧共用同一值，禁止字面量重复。

use mongodb::{
    bson::{doc, Document},
    options::IndexOptions,
    Database, IndexModel,
};

use crate::repository::extensions::SupplierExt;
use crate::Result;

/// `supplier_account` 集合名。
pub(crate) const SUPPLIER_ACCOUNTS: &str = <mongodb::Database as SupplierExt>::SUPPLIER_ACCOUNTS;
/// `supplier_commercial_profile_revision` 集合名。
pub(crate) const SUPPLIER_COMMERCIAL_PROFILE_REVISIONS: &str =
    <mongodb::Database as SupplierExt>::SUPPLIER_COMMERCIAL_PROFILE_REVISIONS;
/// `supplier_capability` 集合名。
pub(crate) const SUPPLIER_CAPABILITIES: &str = <mongodb::Database as SupplierExt>::SUPPLIER_CAPABILITIES;
/// `supplier_capability_revision` 集合名。
pub(crate) const SUPPLIER_CAPABILITY_REVISIONS: &str =
    <mongodb::Database as SupplierExt>::SUPPLIER_CAPABILITY_REVISIONS;
/// `supplier_qualification` 集合名。
pub(crate) const SUPPLIER_QUALIFICATIONS: &str = <mongodb::Database as SupplierExt>::SUPPLIER_QUALIFICATIONS;
/// `supplier_qualification_revision` 集合名。
pub(crate) const SUPPLIER_QUALIFICATION_REVISIONS: &str =
    <mongodb::Database as SupplierExt>::SUPPLIER_QUALIFICATION_REVISIONS;
/// `supplier_qualification_capability` 集合名。
pub(crate) const SUPPLIER_QUALIFICATION_CAPABILITIES: &str =
    <mongodb::Database as SupplierExt>::SUPPLIER_QUALIFICATION_CAPABILITIES;
/// `supplier_rating_revision` 集合名。
pub(crate) const SUPPLIER_RATING_REVISIONS: &str =
    <mongodb::Database as SupplierExt>::SUPPLIER_RATING_REVISIONS;
/// `supplier_profile_command` 集合名。
pub(crate) const SUPPLIER_PROFILE_COMMANDS: &str =
    <mongodb::Database as SupplierExt>::SUPPLIER_PROFILE_COMMANDS;

/// 创建本域集合的幂等命名索引。
///
/// 逐条落地数据模型 §6.2「必需约束与索引」：一个 `party` 至多一个供应商
/// 角色；供应商编号唯一；`(supplier_id, revision_no)` 唯一（商务版本）；
/// `(supplier_id, capability_code, revision_no)` 唯一（能力版本）；
/// 供应商/资质类型/证书编号组合唯一；资质 `valid_to + status` 到期预警；
/// 能力 `capability_code + status + valid_to` 选品与到期预警。
///
/// 身份类字段使用**全局唯一索引**（与 accounts 的处理一致）：`supplier_account`/
/// `supplier_capability`/`supplier_qualification` 软删除后仍保留身份，避免
/// 复用破坏恢复与历史单据追溯语义。
///
/// # 参数
/// * `db` - 目标 MongoDB 数据库
///
/// # 错误
/// 当已有数据违反唯一约束或 MongoDB 无法创建索引时返回错误。
pub(crate) async fn ensure(db: &Database) -> Result<()> {
    create_indexes(db, SUPPLIER_ACCOUNTS, supplier_account_indexes()).await?;
    create_indexes(
        db,
        SUPPLIER_COMMERCIAL_PROFILE_REVISIONS,
        commercial_profile_indexes(),
    )
    .await?;
    create_indexes(db, SUPPLIER_CAPABILITIES, capability_indexes()).await?;
    create_indexes(db, SUPPLIER_CAPABILITY_REVISIONS, capability_revision_indexes()).await?;
    create_indexes(db, SUPPLIER_QUALIFICATIONS, qualification_indexes()).await?;
    create_indexes(
        db,
        SUPPLIER_QUALIFICATION_REVISIONS,
        qualification_revision_indexes(),
    )
    .await?;
    create_indexes(
        db,
        SUPPLIER_QUALIFICATION_CAPABILITIES,
        qualification_capability_indexes(),
    )
    .await?;
    create_indexes(db, SUPPLIER_RATING_REVISIONS, rating_revision_indexes()).await?;
    create_indexes(db, SUPPLIER_PROFILE_COMMANDS, profile_command_indexes()).await?;
    Ok(())
}

/// 为单个集合创建一组幂等命名索引。
async fn create_indexes(db: &Database, collection: &str, indexes: Vec<IndexModel>) -> Result<()> {
    db.collection::<Document>(collection)
        .create_indexes(indexes)
        .await?;
    Ok(())
}

/// 返回 `supplier_account` 的身份约束和列表查询索引。
///
/// `uk_supplier_accounts_id` 覆盖业务主键 `id` 的精确与 `$in` 批量读取
/// （PROC-R10：`current_legal_names_by_account_ids` 经 `supplier_party_refs`
/// 的 `active_ids_filter` 按 `id $in` 批量取关联，默认 `_id` 索引不能覆盖
/// 业务字段 `id`，此前只能集合扫描）。
/// 迁移：先运行 `SupplierRepository::duplicate_supplier_account_ids` 确认无重复
/// `id`，再执行幂等 `ensure` 创建索引，最后用 `explain` 验证 `$in` 命中
/// `uk_supplier_accounts_id` 且无 `COLLSCAN`。
/// 回滚：删除 `uk_supplier_accounts_id`，批量查询退化为集合扫描，不改变数据。
/// 失败关闭：存量存在重复 `id` 时 `ensure` 返回唯一冲突错误，部署必须中止，
/// 先按审计诊断清理重复后再重跑，禁止跳过审计强行建索引。
fn supplier_account_indexes() -> Vec<IndexModel> {
    vec![
        unique_index("uk_supplier_accounts_id", doc! { "id": 1 }),
        unique_index("uk_supplier_accounts_party", doc! { "party_id": 1 }),
        unique_index("uk_supplier_accounts_supplier_no", doc! { "supplier_no": 1 }),
        named_index("idx_supplier_accounts_status", doc! { "status": 1 }),
    ]
}

/// 返回 `supplier_commercial_profile_revision` 的版本唯一约束索引。
fn commercial_profile_indexes() -> Vec<IndexModel> {
    vec![unique_index(
        "uk_supplier_commercial_profile_revisions_supplier_revision",
        doc! { "supplier_id": 1, "revision_no": 1 },
    )]
}

/// 返回 `supplier_capability` 的身份约束与选品/到期预警索引。
fn capability_indexes() -> Vec<IndexModel> {
    vec![
        unique_index(
            "uk_supplier_capabilities_supplier_code",
            doc! { "supplier_id": 1, "capability_code": 1 },
        ),
        named_index(
            "idx_supplier_capabilities_selection",
            doc! { "capability_code": 1, "status": 1, "valid_to": 1 },
        ),
        named_index(
            "idx_supplier_capabilities_supplier_status",
            doc! { "supplier_id": 1, "status": 1 },
        ),
    ]
}

/// 返回 `supplier_capability_revision` 的版本唯一约束（追加式修订）。
fn capability_revision_indexes() -> Vec<IndexModel> {
    vec![unique_index(
        "uk_supplier_capability_revisions_identity",
        doc! { "supplier_id": 1, "capability_code": 1, "revision_no": 1 },
    )]
}

/// 返回 `supplier_qualification` 的身份约束与到期预警索引。
fn qualification_indexes() -> Vec<IndexModel> {
    vec![
        unique_index(
            "uk_supplier_qualifications_identity",
            doc! { "supplier_id": 1, "qualification_type": 1, "certificate_no": 1 },
        ),
        named_index(
            "idx_supplier_qualifications_expiry",
            doc! { "valid_to": 1, "status": 1 },
        ),
        named_index(
            "idx_supplier_qualifications_supplier",
            doc! { "supplier_id": 1, "qualification_type": 1, "status": 1 },
        ),
        named_index(
            "idx_supplier_qualifications_list_filter",
            doc! { "qualification_type": 1, "status": 1, "valid_to": 1 },
        ),
    ]
}

/// 返回 `supplier_qualification_revision` 的版本唯一约束（追加式修订）。
fn qualification_revision_indexes() -> Vec<IndexModel> {
    vec![unique_index(
        "uk_supplier_qualification_revisions_identity",
        doc! {
            "supplier_id": 1,
            "qualification_type": 1,
            "certificate_no": 1,
            "revision_no": 1,
        },
    )]
}

/// 返回 `supplier_qualification_capability` 的关联唯一约束。
///
/// 资质 ↔ 能力是纯关联行（§6.2 明确适用能力），同一对关联重复写入属于
/// 事实行重复，由唯一索引兜底拒绝。
fn qualification_capability_indexes() -> Vec<IndexModel> {
    vec![unique_index(
        "uk_supplier_qualification_capabilities_link",
        doc! { "qualification_id": 1, "capability_id": 1 },
    )]
}

/// 返回 `supplier_rating_revision` 的版本唯一约束与历史查询索引。
fn rating_revision_indexes() -> Vec<IndexModel> {
    vec![
        unique_index(
            "uk_supplier_rating_revisions_supplier_revision",
            doc! { "supplier_id": 1, "revision_no": 1 },
        ),
        named_index(
            "idx_supplier_rating_revisions_history",
            doc! { "supplier_id": 1, "valid_from": 1, "valid_to": 1 },
        ),
    ]
}

/// 返回根级保存命令的幂等唯一约束。
fn profile_command_indexes() -> Vec<IndexModel> {
    vec![unique_index(
        "uk_supplier_profile_commands_idempotency_key",
        doc! { "idempotency_key": 1 },
    )]
}

/// 构建命名普通索引。
fn named_index(name: impl Into<String>, keys: Document) -> IndexModel {
    IndexModel::builder()
        .keys(keys)
        .options(IndexOptions::builder().name(name.into()).build())
        .build()
}

/// 构建命名唯一索引。
fn unique_index(name: impl Into<String>, keys: Document) -> IndexModel {
    IndexModel::builder()
        .keys(keys)
        .options(IndexOptions::builder().name(name.into()).unique(true).build())
        .build()
}

#[cfg(test)]
mod tests {
    use mongodb::bson::doc;

    use super::{
        capability_indexes, commercial_profile_indexes, qualification_capability_indexes,
        qualification_indexes, rating_revision_indexes, supplier_account_indexes,
    };

    #[test]
    fn supplier_account_identity_indexes_are_globally_unique() {
        let indexes = supplier_account_indexes();

        for name in [
            "uk_supplier_accounts_id",
            "uk_supplier_accounts_party",
            "uk_supplier_accounts_supplier_no",
        ] {
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
    fn supplier_account_id_index_covers_batch_lookups() {
        let index = supplier_account_indexes()
            .into_iter()
            .find(|index| {
                index.options.as_ref().and_then(|options| options.name.as_deref())
                    == Some("uk_supplier_accounts_id")
            })
            .unwrap();
        assert_eq!(index.keys, doc! { "id": 1 });
        assert_eq!(
            index.options.as_ref().and_then(|options| options.unique),
            Some(true)
        );
    }

    #[test]
    fn commercial_profile_indexes_cover_revision_identity() {
        let indexes = commercial_profile_indexes();

        assert!(indexes.iter().any(|index| {
            index.options.as_ref().and_then(|options| options.name.as_deref())
                == Some("uk_supplier_commercial_profile_revisions_supplier_revision")
                && index.keys == doc! { "supplier_id": 1, "revision_no": 1 }
                && index.options.as_ref().and_then(|options| options.unique) == Some(true)
        }));
    }

    #[test]
    fn capability_indexes_cover_identity_and_selection() {
        let indexes = capability_indexes();

        assert!(indexes.iter().any(|index| {
            index.keys
                == doc! {
                    "supplier_id": 1,
                    "capability_code": 1,
                }
                && index.options.as_ref().and_then(|options| options.unique) == Some(true)
        }));
        assert!(indexes.iter().any(|index| {
            index.keys
                == doc! {
                    "capability_code": 1,
                    "status": 1,
                    "valid_to": 1,
                }
        }));
    }

    #[test]
    fn qualification_indexes_cover_identity_and_expiry_warning() {
        let indexes = qualification_indexes();

        assert!(indexes.iter().any(|index| {
            index.keys
                == doc! {
                    "supplier_id": 1,
                    "qualification_type": 1,
                    "certificate_no": 1,
                }
                && index.options.as_ref().and_then(|options| options.unique) == Some(true)
        }));
        assert!(indexes
            .iter()
            .any(|index| { index.keys == doc! { "valid_to": 1, "status": 1 } }));
    }

    #[test]
    fn link_and_rating_indexes_are_registered() {
        let links = qualification_capability_indexes();
        assert!(links.iter().any(|index| {
            index.keys == doc! { "qualification_id": 1, "capability_id": 1 }
                && index.options.as_ref().and_then(|options| options.unique) == Some(true)
        }));

        let ratings = rating_revision_indexes();
        assert!(ratings.iter().any(|index| {
            index.keys == doc! { "supplier_id": 1, "revision_no": 1 }
                && index.options.as_ref().and_then(|options| options.unique) == Some(true)
        }));
    }
}
