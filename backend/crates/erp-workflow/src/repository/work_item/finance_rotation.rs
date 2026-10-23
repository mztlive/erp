//! 财务领域拥有轮次集合，版本比较与事务写入复用持久化机制。
pub(crate) use cursor::DispatchRotation;
use mongodb::Database;
use persistence_core::{Executor, rotation as cursor};

use crate::Result;
const COLLECTION: &str = "finance_dispatch_rotation";
/// 读取当前池位置，不推进轮次。
/// # 参数
/// 数据库、规则 ID 与原执行器。
/// # 返回
/// 当前池位置。
/// # 错误
/// 仓储读取失败时返回错误。
pub(crate) async fn rotation(
    db: &Database,
    rule_id: &str,
    executor: &mut dyn Executor,
) -> Result<DispatchRotation> {
    Ok(cursor::rotation(db, COLLECTION, rule_id, executor).await?)
}
/// 在原事务中比较版本并更新位置。
/// # 参数
/// 数据库、规则 ID、预期位置、新位置与原执行器。
/// # 返回
/// 成功返回空值。
/// # 错误
/// 缺少事务、溢出或并发冲突时返回错误。
pub(crate) async fn advance(
    db: &Database,
    rule_id: &str,
    expected: &DispatchRotation,
    last_owner_user_id: Option<&str>,
    executor: &mut dyn Executor,
) -> Result<()> {
    Ok(cursor::advance(db, COLLECTION, rule_id, expected, last_owner_user_id, executor).await?)
}
