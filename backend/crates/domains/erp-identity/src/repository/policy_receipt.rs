//! 授权文件命令回执的集合归属。
use mongodb::Database;
use persistence_core::Repository;

use crate::entity::authorization_bundle::receipt::PolicyReceipt;

pub(crate) const POLICY_RECEIPTS: &str = "authorization_policy_receipts";

/// 提供唯一命令回执仓储，所有访问使用调用方事务。
/// # 参数
/// db 为身份领域数据库。
/// # 返回
/// 授权配置回执集合仓储。
/// # 错误
/// 构造无 I/O；读取与保存错误由仓储方法返回。
pub(crate) fn receipts(db: &Database) -> Repository<'_, PolicyReceipt> {
    Repository::new(db, POLICY_RECEIPTS)
}
