//! 域 D06 `access_control`：`role`、`permission`、`user_role`、`data_scope`、`audit_event`。
//!
//! P0 从 `repository/extensions.rs` 整体迁入既有访问器（accounts/audit_logs/roles），
//! **调用点签名保持不变**；后续增补（permission/user_role/data_scope/audit_event）
//! 写入本文件。新集合的集合名常量定义为 trait 关联常量（唯一权威来源，
//! conventions §4.3「Repository 与索引共用同一常量」），`indexes/` 与
//! `repository/` 两侧统一取 `<mongodb::Database as AccessControlExt>::` 值。

use entities::access_control::{AuditEvent, DataScope, Permission, UserRole};
use mongodb::Database;

use super::super::access_control::{
    AccessControlRepository, AuditEventFilter, DataScopeFilter, PermissionFilter,
};
use crate::Repository;

/// 访问控制域仓储访问器。
pub trait AccessControlExt {
    /// `permission` 集合名。
    const PERMISSIONS: &'static str = "permissions";
    /// `user_role` 集合名。
    const USER_ROLES: &'static str = "user_roles";
    /// `data_scope` 集合名。
    const DATA_SCOPES: &'static str = "data_scopes";
    /// `audit_event` 集合名。
    const AUDIT_EVENTS: &'static str = "audit_events";

    /// 权限定义列表筛选条件类型（定义见 `repository::access_control`）。
    type PermissionFilter;

    /// 数据范围列表筛选条件类型（定义见 `repository::access_control`）。
    type DataScopeFilter;

    /// 审计事件列表筛选条件类型（定义见 `repository::access_control`）。
    type AuditEventFilter;

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

    /// 获取 `permission` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::access_control::Permission>`。
    fn permissions(&self) -> Repository<'_, Permission>;

    /// 获取 `user_role` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::access_control::UserRole>`。
    fn user_roles(&self) -> Repository<'_, UserRole>;

    /// 获取 `data_scope` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::access_control::DataScope>`。
    fn data_scopes(&self) -> Repository<'_, DataScope>;

    /// 获取 `audit_event` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::access_control::AuditEvent>`。
    fn audit_events(&self) -> Repository<'_, AuditEvent>;

    /// 获取承载跨集合事务写入的域专用仓储。
    ///
    /// # 返回
    /// 返回 `AccessControlRepository` 实例。
    fn access_control(&self) -> AccessControlRepository<'_>;
}

impl AccessControlExt for Database {
    type PermissionFilter = PermissionFilter;
    type DataScopeFilter = DataScopeFilter;
    type AuditEventFilter = AuditEventFilter;

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

    fn permissions(&self) -> Repository<'_, Permission> {
        Repository::new(self, Self::PERMISSIONS)
    }

    fn user_roles(&self) -> Repository<'_, UserRole> {
        Repository::new(self, Self::USER_ROLES)
    }

    fn data_scopes(&self) -> Repository<'_, DataScope> {
        Repository::new(self, Self::DATA_SCOPES)
    }

    fn audit_events(&self) -> Repository<'_, AuditEvent> {
        Repository::new(self, Self::AUDIT_EVENTS)
    }

    fn access_control(&self) -> AccessControlRepository<'_> {
        AccessControlRepository::new(self)
    }
}
