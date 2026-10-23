//! 启动时将单人规则和多行默认规则迁为一规则一池；发布期间必须停止旧版写入。
use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use futures_util::TryStreamExt;
use mongodb::Database;
use mongodb::bson::{Document, doc};
use persistence_core::rotation::migrate_scalar_array;
use persistence_core::{Error, Result};
use serde::Deserialize;

use super::super::extensions::ProcurementResponsibilityExt;

#[derive(Deserialize)]
struct LegacyPool {
    id: String,
    owner_user_ids: Vec<String>,
}

/// 将旧字段迁为负责人池，合并启用默认规则并继承原轮转位置。
/// # 参数
/// `db` 为启动阶段数据库，不允许旧版同时写入。
/// # 返回
/// 迁移可重入；已有新格式成员与轮转位置不重置。
/// # 错误
/// 旧资料异常、池超限、唯一冲突或数据库失败时阻止启动。
pub(crate) async fn migrate_pools(db: &Database) -> Result<()> {
    let rules = db
        .collection::<Document>(<Database as ProcurementResponsibilityExt>::PROCUREMENT_RESPONSIBILITY_RULES);
    migrate_scalar_array(
        db,
        <Database as ProcurementResponsibilityExt>::PROCUREMENT_RESPONSIBILITY_RULES,
        "owner_user_id",
        "owner_user_ids",
    )
    .await?;
    rules
        .update_many(
            doc! { "strategy": { "$exists": false } },
            doc! { "$set": { "strategy": "ROUND_ROBIN" } },
        )
        .await?;
    merge_defaults(db).await?;
    rules
        .update_many(
            doc! { "rule_type": "DEFAULT_DISPATCHER" },
            doc! { "$set": { "selector_key": "default_dispatcher" } },
        )
        .await?;
    rules
        .update_many(
            doc! { "owner_user_id": { "$exists": true } },
            doc! { "$unset": { "owner_user_id": "" } },
        )
        .await?;
    Ok(())
}

/// 合并前先保存完整成员，再停用被合并规则，最后由调用方统一收敛选择器键。
async fn merge_defaults(db: &Database) -> Result<()> {
    let rules = db.collection::<LegacyPool>(
        <Database as ProcurementResponsibilityExt>::PROCUREMENT_RESPONSIBILITY_RULES,
    );
    let defaults: Vec<LegacyPool> = rules.find(doc! { "rule_type": "DEFAULT_DISPATCHER", "status": "active", "deleted_at": NOT_DELETED_TIMESTAMP_BSON })
        .sort(doc! { "id": 1 }).await?.try_collect().await?;
    let Some(first) = defaults.first() else {
        return Ok(());
    };
    let owners = merged_owners(&defaults)?;
    let last = legacy_last_owner(db).await?;
    rules.update_one(doc! { "id": &first.id }, doc! { "$set": { "owner_user_ids": owners } }).await?;
    let others: Vec<&str> = defaults.iter().skip(1).map(|rule| rule.id.as_str()).collect();
    if !others.is_empty() {
        rules
            .update_many(
                doc! { "id": { "$in": others } },
                doc! { "$set": { "status": "disabled", "migration_merged_into": &first.id } },
            )
            .await?;
    }
    db.collection::<Document>("procurement_dispatch_rotation")
        .update_one(
            doc! { "_id": &first.id },
            doc! { "$setOnInsert": { "last_owner_user_id": last, "revision": 0_i64 } },
        )
        .upsert(true)
        .await?;
    Ok(())
}

/// 从旧全局轮次反查最近成员；已经迁移的新池位置由 setOnInsert 保留。
async fn legacy_last_owner(db: &Database) -> Result<Option<String>> {
    let old = db
        .collection::<Document>("procurement_dispatch_rotation")
        .find_one(doc! { "_id": "default" })
        .await?;
    let Some(id) = old.as_ref().and_then(|value| value.get_str("last_rule_id").ok()) else {
        return Ok(None);
    };
    let rule = db
        .collection::<LegacyPool>(
            <Database as ProcurementResponsibilityExt>::PROCUREMENT_RESPONSIBILITY_RULES,
        )
        .find_one(doc! { "id": id })
        .await?;
    Ok(rule.and_then(|rule| rule.owner_user_ids.into_iter().next()))
}

/// 保留旧规则顺序与既有池成员顺序，合并重试不得丢人或增加权重。
fn merged_owners(rules: &[LegacyPool]) -> Result<Vec<String>> {
    let mut owners = Vec::new();
    for owner in rules.iter().flat_map(|rule| &rule.owner_user_ids) {
        if !owners.contains(owner) {
            owners.push(owner.clone());
        }
    }
    if owners.is_empty() || owners.len() > 100 {
        return Err(Error::UnsupportedDeployment("默认负责人池须包含 1 至 100 人，请维护旧规则后重新启动"));
    }
    Ok(owners)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn merging_is_ordered_and_safe_to_repeat_after_partial_migration() {
        let rule = |id: &str, names: &[&str]| LegacyPool {
            id: id.into(),
            owner_user_ids: names.iter().map(|name| name.to_string()).collect(),
        };
        assert_eq!(merged_owners(&[rule("a", &["u1"]), rule("b", &["u2"])]).unwrap(), ["u1", "u2"]);
        assert_eq!(merged_owners(&[rule("a", &["u1", "u2"]), rule("b", &["u2"])]).unwrap(), ["u1", "u2"]);
        assert!(merged_owners(&[rule("empty", &[])]).is_err());
    }
}
