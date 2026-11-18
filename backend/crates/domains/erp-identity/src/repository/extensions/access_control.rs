//! 身份域集合访问器；集合名由 trait 关联常量统一定义，仓储与索引共用。

use mongodb::Database;

use crate::repository::owned::{
    AccountCoreRepository, AuditEventRepository, PermissionRepository, PersonDataScopeRepository,
    PersonQueryQualificationRepository, RoleRepository, UserRoleRepository,
};

/// 访问控制域仓储访问器。
pub trait AccessControlExt {
    /// 人员唯一有效范围集合。
    const PERSON_DATA_SCOPES: &'static str = "person_data_scopes";
    /// 获取人员范围仓储。
    /// 返回 `PersonDataScopeRepository`。
    fn person_data_scopes(&self) -> PersonDataScopeRepository<'_>;
    /// `permission` 集合名。
    const PERMISSIONS: &'static str = "permissions";
    /// `user_role` 集合名。
    const USER_ROLES: &'static str = "user_roles";
    /// `audit_event` 集合名。
    const AUDIT_EVENTS: &'static str = "audit_events";
    /// 人员查询资格集合名。
    const PERSON_QUERY_QUALIFICATIONS: &'static str = "person_query_qualifications";

    /// 获取统一账号仓储。
    /// 返回 `AccountCoreRepository`。
    fn accounts(&self) -> AccountCoreRepository<'_>;

    /// 获取角色仓储。
    /// 返回 `RoleRepository`。
    fn roles(&self) -> RoleRepository<'_>;

    /// 获取 `permission` 集合的仓储。
    /// 返回 `PermissionRepository`。
    fn permissions(&self) -> PermissionRepository<'_>;

    /// 获取 `user_role` 集合的仓储。
    /// 返回 `UserRoleRepository`。
    fn user_roles(&self) -> UserRoleRepository<'_>;

    /// 获取 `audit_event` 集合的仓储。
    /// 返回 `AuditEventRepository`。
    fn audit_events(&self) -> AuditEventRepository<'_>;

    /// 获取人员查询资格仓储。
    /// 返回 `PersonQueryQualificationRepository`。
    fn person_query_qualifications(&self) -> PersonQueryQualificationRepository<'_>;
}

impl AccessControlExt for Database {
    fn person_data_scopes(&self) -> PersonDataScopeRepository<'_> {
        PersonDataScopeRepository::new(self, Self::PERSON_DATA_SCOPES)
    }
    // 获取统一账号Repository，固定打开集合 `accounts`。
    // 返回 `AccountCoreRepository<'_>` 结果。
    fn accounts(&self) -> AccountCoreRepository<'_> {
        AccountCoreRepository::new(self, "accounts")
    }

    // 获取角色 Repository，固定打开集合 `roles`。
    // 返回角色仓储。
    fn roles(&self) -> RoleRepository<'_> {
        RoleRepository::new(self, "roles")
    }

    fn permissions(&self) -> PermissionRepository<'_> {
        PermissionRepository::new(self, Self::PERMISSIONS)
    }

    fn user_roles(&self) -> UserRoleRepository<'_> {
        UserRoleRepository::new(self, Self::USER_ROLES)
    }

    fn audit_events(&self) -> AuditEventRepository<'_> {
        AuditEventRepository::new(self, Self::AUDIT_EVENTS)
    }

    fn person_query_qualifications(&self) -> PersonQueryQualificationRepository<'_> {
        PersonQueryQualificationRepository::new(self, Self::PERSON_QUERY_QUALIFICATIONS)
    }
}
