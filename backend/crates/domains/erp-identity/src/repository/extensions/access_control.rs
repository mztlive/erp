//! 域 D06 `access_control`：`role`、`permission`、`user_role`、`data_scope`、`audit_event`。
//!
//! P0 从 `repository/extensions.rs` 整体迁入既有访问器（accounts/audit_logs/roles），
//! **调用点签名保持不变**；后续增补（permission/user_role/data_scope/audit_event）
//! 写入本文件。新集合的集合名常量定义为 trait 关联常量（唯一权威来源，
//! conventions §4.3「Repository 与索引共用同一常量」），`indexes/` 与
//! `repository/` 两侧统一取 `<mongodb::Database as AccessControlExt>::` 值。

use mongodb::Database;

use crate::repository::access_control::{AuditEventFilter, DataScopeFilter, PermissionFilter};
use crate::repository::owned::{
    AccountCoreRepository, AuditEventRepository, DataScopeRepository, PermissionRepository,
    PersonDataScopeRepository, PersonQueryQualificationRepository, PersonalBusinessGrantRepository,
    RoleRepository, UserRoleRepository,
};

/// 访问控制域仓储访问器。
pub trait AccessControlExt {
    /// 人员唯一有效范围集合。
    const PERSON_DATA_SCOPES: &'static str = "person_data_scopes";
    /// 获取人员范围仓储。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回 `PersonDataScopeRepository`。
    ///
    /// # 错误
    /// 不返回错误。
    fn person_data_scopes(&self) -> PersonDataScopeRepository<'_>;
    /// 独立个人业务扩展授权集合。
    const PERSONAL_BUSINESS_GRANTS: &'static str = "personal_business_grants";
    /// 获取个人业务授权仓储。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回本领域个人业务授权集合仓储。
    ///
    /// # 错误
    /// 不返回错误。
    fn personal_business_grants(&self) -> PersonalBusinessGrantRepository<'_>;
    /// `permission` 集合名。
    const PERMISSIONS: &'static str = "permissions";
    /// `user_role` 集合名。
    const USER_ROLES: &'static str = "user_roles";
    /// `data_scope` 集合名。
    const DATA_SCOPES: &'static str = "data_scopes";
    /// `audit_event` 集合名。
    const AUDIT_EVENTS: &'static str = "audit_events";
    /// 人员查询资格集合名。
    const PERSON_QUERY_QUALIFICATIONS: &'static str = "person_query_qualifications";

    /// 权限定义列表筛选条件类型（定义见 `repository::access_control`）。
    type PermissionFilter;

    /// 数据范围列表筛选条件类型（定义见 `repository::access_control`）。
    type DataScopeFilter;

    /// 审计事件列表筛选条件类型（定义见 `repository::access_control`）。
    type AuditEventFilter;

    /// 获取统一账号仓储。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回 `AccountCoreRepository`。
    ///
    /// # 错误
    /// 不返回错误。
    fn accounts(&self) -> AccountCoreRepository<'_>;

    /// 获取角色仓储。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回 `RoleRepository`。
    ///
    /// # 错误
    /// 不返回错误。
    fn roles(&self) -> RoleRepository<'_>;

    /// 获取 `permission` 集合的仓储。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回 `PermissionRepository`。
    ///
    /// # 错误
    /// 不返回错误。
    fn permissions(&self) -> PermissionRepository<'_>;

    /// 获取 `user_role` 集合的仓储。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回 `UserRoleRepository`。
    ///
    /// # 错误
    /// 不返回错误。
    fn user_roles(&self) -> UserRoleRepository<'_>;

    /// 获取 `data_scope` 集合的仓储。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回 `DataScopeRepository`。
    ///
    /// # 错误
    /// 不返回错误。
    fn data_scopes(&self) -> DataScopeRepository<'_>;

    /// 获取 `audit_event` 集合的仓储。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回 `AuditEventRepository`。
    ///
    /// # 错误
    /// 不返回错误。
    fn audit_events(&self) -> AuditEventRepository<'_>;

    /// 获取人员查询资格仓储。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回 `PersonQueryQualificationRepository`。
    ///
    /// # 错误
    /// 不返回错误。
    fn person_query_qualifications(&self) -> PersonQueryQualificationRepository<'_>;
}

impl AccessControlExt for Database {
    fn person_data_scopes(&self) -> PersonDataScopeRepository<'_> {
        PersonDataScopeRepository::new(self, Self::PERSON_DATA_SCOPES)
    }
    fn personal_business_grants(&self) -> PersonalBusinessGrantRepository<'_> {
        PersonalBusinessGrantRepository::new(self, Self::PERSONAL_BUSINESS_GRANTS)
    }
    type PermissionFilter = PermissionFilter;
    type DataScopeFilter = DataScopeFilter;
    type AuditEventFilter = AuditEventFilter;

    /// 获取统一账号Repository，固定打开集合 `accounts`。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回 `AccountCoreRepository<'_>` 结果。
    ///
    /// # 错误
    /// 不返回错误。
    fn accounts(&self) -> AccountCoreRepository<'_> {
        AccountCoreRepository::new(self, "accounts")
    }

    /// 获取角色 Repository，固定打开集合 `roles`。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回角色仓储。
    ///
    /// # 错误
    /// 不返回错误。
    fn roles(&self) -> RoleRepository<'_> {
        RoleRepository::new(self, "roles")
    }

    fn permissions(&self) -> PermissionRepository<'_> {
        PermissionRepository::new(self, Self::PERMISSIONS)
    }

    fn user_roles(&self) -> UserRoleRepository<'_> {
        UserRoleRepository::new(self, Self::USER_ROLES)
    }

    fn data_scopes(&self) -> DataScopeRepository<'_> {
        DataScopeRepository::new(self, Self::DATA_SCOPES)
    }

    fn audit_events(&self) -> AuditEventRepository<'_> {
        AuditEventRepository::new(self, Self::AUDIT_EVENTS)
    }

    fn person_query_qualifications(&self) -> PersonQueryQualificationRepository<'_> {
        PersonQueryQualificationRepository::new(self, Self::PERSON_QUERY_QUALIFICATIONS)
    }
}
