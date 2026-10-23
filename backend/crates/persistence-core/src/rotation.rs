//! 调用方拥有集合的版本化轮转游标；仅提供事务读取和比较写入机制。

use mongodb::Database;
use mongodb::bson::{Document, doc};
use serde::{Deserialize, Serialize};

use crate::{Error, Executor, Result, mongo_ops};

/// 单条规则负责人池的持久化位置；没有历史记录时从名单首位开始。
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct DispatchRotation {
    /// 兼容既有轮转记录的游标位置。
    pub last_owner_user_id: Option<String>,
    /// 用于比较写入的并发版本。
    pub revision: i64,
}

impl DispatchRotation {
    fn update_document(&self, position: Option<&str>) -> Result<Document> {
        let revision =
            self.revision.checked_add(1).ok_or(Error::EntityMetadataOutOfRange("游标版本超出范围"))?;
        Ok(doc! { "$set": { "last_owner_user_id": position, "revision": revision } })
    }
}

/// 读取轮转位置，不创建记录或推进轮次。
/// # 参数
/// `db` 为领域数据库，`executor` 为调用方执行器。
/// # 返回
/// 返回当前位置；首次使用返回空位置和版本零。
/// # 错误
/// 数据读取或反序列化失败时返回错误。
pub async fn rotation(
    db: &Database,
    collection: &str,
    rule_id: &str,
    executor: &mut dyn Executor,
) -> Result<DispatchRotation> {
    Ok(mongo_ops::find_one(&db.collection::<DispatchRotation>(collection), doc! { "_id": rule_id }, executor)
        .await?
        .unwrap_or_default())
}

/// 在原事务中比较版本并记录分派位置，规则维护可保留原位置仅更新版本。
/// # 参数
/// `expected` 为本事务读取的位置；`last_owner_user_id` 为分派规则或保留的原位置。
/// # 返回
/// 写入成功返回空值；由外层事务统一提交。
/// # 错误
/// 缺少事务、版本溢出、并发冲突或仓储失败时拒绝整个调用。
pub async fn advance(
    db: &Database,
    collection: &str,
    rule_id: &str,
    expected: &DispatchRotation,
    last_owner_user_id: Option<&str>,
    executor: &mut dyn Executor,
) -> Result<()> {
    if executor.session().is_none() {
        return Err(Error::UnsupportedDeployment("游标写入必须加入调用方事务"));
    }
    let update = expected.update_document(last_owner_user_id)?;
    mongo_ops::update_one(
        &db.collection::<DispatchRotation>(collection),
        doc! { "_id": rule_id, "revision": expected.revision },
        update,
        true,
        executor,
    )
    .await
    .map_err(|error| match error {
        Error::DuplicateKey(_) => Error::OptimisticLockingError,
        other => other,
    })?;
    Ok(())
}

/// 将旧标量字段迁为单元素数组，已有数组保持不变。
/// # 参数
/// 数据库、集合名及新旧字段名；执行期间须停止旧版写入。
/// # 返回
/// 可重入地转换字段，保留旧字段供拥有领域完成后续迁移。
/// # 错误
/// 数据库失败时返回错误，调用方应阻止启动。
pub async fn migrate_scalar_array(db: &Database, collection: &str, scalar: &str, array: &str) -> Result<()> {
    db.collection::<Document>(collection)
        .update_many(
            doc! { array: { "$exists": false }, scalar: { "$type": "string" } },
            vec![doc! { "$set": { array: [format!("${scalar}")] } }],
        )
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn advancing_and_configuration_guard_share_revision_without_resetting_position() {
        let initial = DispatchRotation::default();
        let assigned = initial.update_document(Some("member-a")).unwrap();
        let assigned = assigned.get_document("$set").unwrap();
        assert_eq!(assigned.get_str("last_owner_user_id").unwrap(), "member-a");
        assert_eq!(assigned.get_i64("revision").unwrap(), 1);
        let current = DispatchRotation { last_owner_user_id: Some("member-a".into()), revision: 7 };
        let guarded = current.update_document(current.last_owner_user_id.as_deref()).unwrap();
        let guarded = guarded.get_document("$set").unwrap();
        assert_eq!(guarded.get_str("last_owner_user_id").unwrap(), "member-a");
        assert_eq!(guarded.get_i64("revision").unwrap(), 8);
        assert_eq!(current.revision, 7);
        let invalid = DispatchRotation { revision: i64::MAX, ..Default::default() };
        assert!(matches!(invalid.update_document(Some("member-b")), Err(Error::EntityMetadataOutOfRange(_))));
    }
}
