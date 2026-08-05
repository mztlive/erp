//! 域 D06 `access_control`：`role`、`permission`、`user_role`、`data_scope`、`audit_event`。
//!
//! P0 从 `repository/extensions.rs` 整体迁入既有访问器（accounts/audit_logs/roles），
//! **调用点签名保持不变**；后续增补（data_scope 等）写入本文件。

use mongodb::Database;

use crate::Repository;

/// 访问控制域仓储访问器。
pub trait AccessControlExt {
    /// 获取统一账号Repository
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::AccountCore>` 结果。
    fn accounts(&self) -> Repository<'_, entities::AccountCore>;

    /// 获取审计日志Repository
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::AuditLog>` 结果。
    fn audit_logs(&self) -> Repository<'_, entities::AuditLog>;

    /// 获取角色 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::Role>`。
    fn roles(&self) -> Repository<'_, entities::Role>;
}

impl AccessControlExt for Database {
    /// 获取统一账号Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::AccountCore>` 结果。
    fn accounts(&self) -> Repository<'_, entities::AccountCore> {
        Repository::new(self, "accounts")
    }

    /// 获取审计日志Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::AuditLog>` 结果。
    fn audit_logs(&self) -> Repository<'_, entities::AuditLog> {
        Repository::new(self, "audit_logs")
    }

    /// 获取角色 Repository。
    ///
    /// # 返回
    /// 返回角色仓储。
    fn roles(&self) -> Repository<'_, entities::Role> {
        Repository::new(self, "roles")
    }
}
