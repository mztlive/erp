//! 在 MongoDB `Database` 上实现的审计仓储访问器。

use mongodb::Database;

use crate::repository::owned::AuditLogRepository;

/// 审计日志集合访问器。
pub trait AuditExt {
    /// 审计日志集合名。
    const AUDIT_LOGS: &'static str = "audit_logs";

    /// 返回绑定 `audit_logs` 的审计日志仓储。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回当前数据库上的审计日志仓储。
    ///
    /// # 错误
    /// 不返回错误。
    fn audit_logs(&self) -> AuditLogRepository<'_>;
}

impl AuditExt for Database {
    /// 返回绑定 `AUDIT_LOGS` 的审计日志仓储。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回当前数据库上的审计日志仓储。
    ///
    /// # 错误
    /// 不返回错误。
    fn audit_logs(&self) -> AuditLogRepository<'_> {
        AuditLogRepository::new(self, Self::AUDIT_LOGS)
    }
}
