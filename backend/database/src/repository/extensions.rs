//! MongoDB Database扩展trait
//!
//! 提供便捷的方式访问各个实体的Repository

use mongodb::Database;

use crate::Repository;

/// Database扩展trait，为Database提供便捷的Repository访问方法
pub trait DatabaseExt {
    /// 获取统一账号Repository
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::AccountCore>` 结果。
    fn accounts(&self) -> Repository<'_, entities::AccountCore>;

    /// 获取消费者Repository
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::Consumer>` 结果。
    fn consumers(&self) -> Repository<'_, entities::Consumer>;

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

impl DatabaseExt for Database {
    /// 获取统一账号Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::AccountCore>` 结果。
    fn accounts(&self) -> Repository<'_, entities::AccountCore> {
        Repository::new(self, "accounts")
    }

    /// 获取消费者Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::Consumer>` 结果。
    fn consumers(&self) -> Repository<'_, entities::Consumer> {
        Repository::new(self, "consumers")
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
