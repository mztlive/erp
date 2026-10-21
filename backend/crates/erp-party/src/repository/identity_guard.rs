//! 企业身份写入互斥：同名创建与改名必须在原事务中争用同一唯一记录。

use mongodb::Database;
use mongodb::bson::{Document, doc};
use persistence_core::Executor;

use super::PartyExt;
use super::exact_identity::{ContractIdentityCandidate, contract_identity_candidates};
use crate::{Error, Result};

/// 在原事务中锁定名称，再读取当前主体候选。
///
/// # 参数
/// * `db` / `name` / `credit` - 数据库、已规范化法定名称与信用代码。
/// * `executor` - 创建或修订用例的事务执行器。
/// # 返回
/// 返回同一事务快照下的候选；不自动创建或授予业务权限。
/// # 错误
/// 非事务调用、并发占用、唯一冲突或数据库失败时拒绝。
pub async fn guarded_candidates(
    db: &Database,
    name: &str,
    credit: Option<&str>,
    executor: &mut dyn Executor,
) -> Result<Vec<ContractIdentityCandidate>> {
    lock_name(db, name, executor).await?;
    Ok(contract_identity_candidates(db, name, credit, executor).await?)
}

/// 争用由 MongoDB `_id` 唯一索引保护的名称记录；锁随调用事务提交或回滚。
///
/// # 参数
/// * `db` / `name` / `executor` - 数据库、法定名称及原写入事务。
/// # 返回
/// 成功时该事务已写入名称互斥记录。
/// # 错误
/// 缺事务、空名称、并发写冲突或数据库失败时拒绝；不自动重放。
pub async fn lock_name(db: &Database, name: &str, executor: &mut dyn Executor) -> Result<()> {
    let name = name.trim();
    if name.is_empty() {
        return Err(Error::ValidationError("法定名称不能为空".into()));
    }
    let session = executor.session().ok_or_else(|| Error::Internal("主体身份校验必须位于写事务".into()))?;
    db.collection::<Document>(<Database as PartyExt>::PARTY_IDENTITY_GUARDS)
        .update_one(doc! { "_id": name }, doc! { "$inc": { "generation": 1_i64 } })
        .upsert(true)
        .session(session)
        .await
        .map_err(persistence_core::Error::from)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use mongodb::Client;
    use persistence_core::NoTransaction;

    use super::*;

    /// 在任何数据库操作前拒绝缺少事务的调用。
    #[tokio::test]
    async fn identity_guard_rejects_nontransactional_writers() {
        let db = Client::with_uri_str("mongodb://127.0.0.1:1").await.unwrap().database("unit_test");
        assert!(matches!(lock_name(&db, "共享企业", &mut NoTransaction).await, Err(Error::Internal(_))));
        assert!(matches!(lock_name(&db, "  ", &mut NoTransaction).await, Err(Error::ValidationError(_))));
    }
}
