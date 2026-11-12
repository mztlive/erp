//! 账号资源修订号与来源修改在同一事务提交，不通过扫描授权记录计算版本。
use std::collections::BTreeSet;

use mongodb::Database;
use mongodb::bson::{Document, doc};
use persistence_core::{Executor, mongo_ops};

use crate::entity::resource_access::ResourceKind;
use crate::{Error, Result};

const REVISIONS: &str = "resource_grant_revisions";

/// 读取单个账号资源的修订号；不存在的记录使用初始版本零。
/// # 参数
/// user、resource 为可信授权身份；executor 为调用事务。
/// # 返回
/// 非负修订号，通过 MongoDB 内建 _id 唯一索引读取。
/// # 错误
/// 数据库失败或版本损坏时拒绝。
pub(crate) async fn revision(
    db: &Database,
    user: &str,
    resource: ResourceKind,
    executor: &mut dyn Executor,
) -> Result<u64> {
    let collection = db.collection::<Document>(REVISIONS);
    let row =
        mongo_ops::find_one(&collection, doc! { "_id": revision_id(user, resource)? }, executor).await?;
    row.map(|row| {
        let revision = row.get_i64("revision").map_err(|e| Error::Internal(e.to_string()))?;
        u64::try_from(revision).map_err(|e| Error::Internal(e.to_string()))
    })
    .transpose()
    .map(|value| value.unwrap_or(0))
}

/// 为发生变化的资源递增版本；必须与来源及授权行共享事务。
/// # 参数
/// user 为接收人；resources 为修改前后目标资源的并集；executor 为发布事务。
/// # 返回
/// 全部受影响资源的版本已递增。
/// # 错误
/// 没有事务、唯一键冲突或持久化失败时拒绝并回滚整个发布。
pub(crate) async fn advance(
    db: &Database,
    user: &str,
    resources: &BTreeSet<ResourceKind>,
    executor: &mut dyn Executor,
) -> Result<()> {
    if !executor.boundary().is_some_and(|boundary| boundary.is_snapshot()) {
        return Err(Error::Internal("授权版本更新必须使用快照事务".into()));
    }
    let collection = db.collection::<Document>(REVISIONS);
    for resource in resources {
        mongo_ops::update_one(&collection, doc! { "_id": revision_id(user, *resource)? },
            doc! { "$inc": { "revision": 1_i64 }, "$setOnInsert": { "recipient": user, "resource": resource.code() } },
            true, executor).await?;
    }
    Ok(())
}

/// 结构化主键避免账号与资源代码的拼接碰撞。
fn revision_id(user: &str, resource: ResourceKind) -> Result<String> {
    Ok(serde_json::to_string(&(user, resource))?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn revision_identity_separates_recipient_and_resource() {
        let key = revision_id("u", ResourceKind::SalesOrder).unwrap();
        assert_ne!(key, revision_id("other", ResourceKind::SalesOrder).unwrap());
        assert_ne!(key, revision_id("u", ResourceKind::PurchaseOrder).unwrap());
        let restored: (String, ResourceKind) = serde_json::from_str(&key).unwrap();
        assert_eq!(restored, ("u".into(), ResourceKind::SalesOrder));
    }
}
