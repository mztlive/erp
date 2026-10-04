//! 审计领域拥有的独立尝试记录集合。

use mongodb::Database;
use persistence_core::Repository;

use crate::entity::AuditAttempt;

/// 尝试记录仓储访问器；不能用于命令恢复。
pub trait AuditAttemptExt {
    const AUDIT_ATTEMPTS: &'static str = "audit_attempts";
    /// 返回保留失败、拒绝和未知分类的尝试仓储。
    /// # 参数
    /// 无。
    /// # 返回
    /// 返回当前数据库中的尝试记录仓储。
    /// # 错误
    /// 无。
    fn audit_attempts(&self) -> Repository<'_, AuditAttempt>;
}
impl AuditAttemptExt for Database {
    fn audit_attempts(&self) -> Repository<'_, AuditAttempt> {
        Repository::new(self, Self::AUDIT_ATTEMPTS)
    }
}
