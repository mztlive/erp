//! 账号资源修订号与来源修改在同一事务提交，不通过扫描授权记录计算版本。
use std::collections::BTreeSet;

use mongodb::Database;
use mongodb::bson::{Document, doc};
use persistence_core::{Executor, mongo_ops};

use crate::entity::resource_access::ResourceKind;
use crate::{Error, Result};

const REVISIONS: &str = "resource_grant_revisions";

/// 用调用方事务和 `_id` 索引读取账号在指定资源上的授权修订；未初始化返回 0，版本不是有效的非负整数或读取失败时返回错误。
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

/// 在来源及授权行的同一快照事务内，递增接收人在变更前后涉及资源的授权修订。无事务、唯一键冲突或写入失败时拒绝，由调用方回滚发布。
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

// 将账号和资源结构化编码为修订文档键，避免简单拼接导致碰撞。
fn revision_id(user: &str, resource: ResourceKind) -> Result<String> {
    Ok(serde_json::to_string(&(user, resource))?)
}

#[cfg(test)]
mod tests {
    use super::*;

    // 验证接收账号或资源变化均生成不同修订键，且可无损解码。
    #[test]
    fn revision_identity_separates_recipient_and_resource() {
        let key = revision_id("u", ResourceKind::SalesOrder).unwrap();
        assert_ne!(key, revision_id("other", ResourceKind::SalesOrder).unwrap());
        assert_ne!(key, revision_id("u", ResourceKind::PurchaseOrder).unwrap());
        let restored: (String, ResourceKind) = serde_json::from_str(&key).unwrap();
        assert_eq!(restored, ("u".into(), ResourceKind::SalesOrder));
    }
}
