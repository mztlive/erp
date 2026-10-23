//! 负责人池轮次拥有仓储；与任务或规则维护在同一事务内写入。

use mongodb::Database;
use mongodb::bson::doc;
use persistence_core::{Executor, mongo_ops};
use serde::{Deserialize, Serialize};

use crate::{Error, Result};

const COLLECTION: &str = "procurement_dispatch_rotation";

/// 单条规则负责人池的持久化位置；没有历史记录时从名单首位开始。
#[derive(Debug, Default, Serialize, Deserialize)]
pub(crate) struct DispatchRotation {
    pub last_owner_user_id: Option<String>,
    pub revision: i64,
}

/// 读取轮转位置，不创建记录或推进轮次。
/// # 参数
/// `db` 为领域数据库，`executor` 为调用方执行器。
/// # 返回
/// 返回当前位置；首次使用返回空位置和版本零。
/// # 错误
/// 数据读取或反序列化失败时返回错误。
pub(crate) async fn rotation(
    db: &Database,
    rule_id: &str,
    executor: &mut dyn Executor,
) -> Result<DispatchRotation> {
    Ok(mongo_ops::find_one(&db.collection::<DispatchRotation>(COLLECTION), doc! { "_id": rule_id }, executor)
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
pub(crate) async fn advance(
    db: &Database,
    rule_id: &str,
    expected: &DispatchRotation,
    last_owner_user_id: Option<&str>,
    executor: &mut dyn Executor,
) -> Result<()> {
    if executor.session().is_none() {
        return Err(Error::Internal("负责人池轮转必须加入任务或规则维护事务".into()));
    }
    let revision =
        expected.revision.checked_add(1).ok_or_else(|| Error::Internal("负责人池轮次版本超出范围".into()))?;
    mongo_ops::update_one(
        &db.collection::<DispatchRotation>(COLLECTION),
        doc! { "_id": rule_id, "revision": expected.revision },
        doc! { "$set": { "last_owner_user_id": last_owner_user_id, "revision": revision } },
        true,
        executor,
    )
    .await?;
    Ok(())
}
