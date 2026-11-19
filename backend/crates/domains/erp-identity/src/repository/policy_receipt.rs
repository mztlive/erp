//! 授权文件命令回执的集合归属。
use mongodb::Database;
use persistence_core::Repository;

use crate::entity::authorization_bundle::receipt::PolicyReceipt;

pub(crate) const POLICY_RECEIPTS: &str = "authorization_policy_receipts";

/// 绑定授权配置命令回执集合，构造不访问数据库；后续读写沿用调用方事务并传播仓储错误。
pub(crate) fn receipts(db: &Database) -> Repository<'_, PolicyReceipt> {
    Repository::new(db, POLICY_RECEIPTS)
}
