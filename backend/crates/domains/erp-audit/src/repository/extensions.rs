//! 在 MongoDB `Database` 上实现的审计仓储访问器。

use mongodb::Database;

use crate::repository::owned::AuditLogRepository;

/// 审计日志集合访问器。
pub trait AuditExt {
    /// 审计日志集合名。
    const AUDIT_LOGS: &'static str = "audit_logs";

    /// 取得当前数据库的 `audit_logs` 仓储，供审计写入和授权后的查询复用。
    fn audit_logs(&self) -> AuditLogRepository<'_>;
}

impl AuditExt for Database {
    // 用 `AUDIT_LOGS` 集合名绑定当前数据库，避免各调用方各自选择日志集合。
    fn audit_logs(&self) -> AuditLogRepository<'_> {
        AuditLogRepository::new(self, Self::AUDIT_LOGS)
    }
}
