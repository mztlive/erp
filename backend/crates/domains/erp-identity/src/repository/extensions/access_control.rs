//! 身份域集合访问器；集合名由 trait 关联常量统一定义，仓储与索引共用。

use mongodb::Database;

use crate::repository::owned::{
    AccountCoreRepository, AuditEventRepository, PermissionRepository, PersonDataScopeRepository,
    PersonQueryQualificationRepository, RoleRepository, UserRoleRepository,
};

/// 统一访问身份域集合，集合名供仓储和索引共用。
pub trait AccessControlExt {
    /// 人员唯一有效范围集合。
    const PERSON_DATA_SCOPES: &'static str = "person_data_scopes";
    /// 访问当前人员数据范围配置，不连接旧范围集合。
    fn person_data_scopes(&self) -> PersonDataScopeRepository<'_>;
    /// `permission` 集合名。
    const PERMISSIONS: &'static str = "permissions";
    /// `user_role` 集合名。
    const USER_ROLES: &'static str = "user_roles";
    /// `audit_event` 集合名。
    const AUDIT_EVENTS: &'static str = "audit_events";
    /// 人员查询资格集合名。
    const PERSON_QUERY_QUALIFICATIONS: &'static str = "person_query_qualifications";

    /// 访问后台和门户共用的统一账号仓储。
    fn accounts(&self) -> AccountCoreRepository<'_>;

    /// 访问角色实体仓储，供角色配置及资格取证使用。
    fn roles(&self) -> RoleRepository<'_>;

    /// 访问权限定义仓储，供目录和管理查询使用。
    fn permissions(&self) -> PermissionRepository<'_>;

    /// 访问用户角色绑定仓储，供角色管理和历史查询使用。
    fn user_roles(&self) -> UserRoleRepository<'_>;

    /// 访问追加式审计事件仓储，供落库和留痕查询使用。
    fn audit_events(&self) -> AuditEventRepository<'_>;

    /// 访问按账号和目录类别配置的人员查询资格仓储。
    fn person_query_qualifications(&self) -> PersonQueryQualificationRepository<'_>;
}

impl AccessControlExt for Database {
    // 将人员范围仓储绑定到当前配置集合。
    fn person_data_scopes(&self) -> PersonDataScopeRepository<'_> {
        PersonDataScopeRepository::new(self, Self::PERSON_DATA_SCOPES)
    }
    // 将统一账号仓储固定绑定到 `accounts` 集合。
    fn accounts(&self) -> AccountCoreRepository<'_> {
        AccountCoreRepository::new(self, "accounts")
    }

    // 将角色实体仓储固定绑定到 `roles` 集合。
    fn roles(&self) -> RoleRepository<'_> {
        RoleRepository::new(self, "roles")
    }

    // 按共享集合名构造权限定义仓储。
    fn permissions(&self) -> PermissionRepository<'_> {
        PermissionRepository::new(self, Self::PERMISSIONS)
    }

    // 按共享集合名构造用户角色绑定仓储。
    fn user_roles(&self) -> UserRoleRepository<'_> {
        UserRoleRepository::new(self, Self::USER_ROLES)
    }

    // 按共享集合名构造审计事件仓储。
    fn audit_events(&self) -> AuditEventRepository<'_> {
        AuditEventRepository::new(self, Self::AUDIT_EVENTS)
    }

    // 按共享集合名构造人员查询资格仓储。
    fn person_query_qualifications(&self) -> PersonQueryQualificationRepository<'_> {
        PersonQueryQualificationRepository::new(self, Self::PERSON_QUERY_QUALIFICATIONS)
    }
}
