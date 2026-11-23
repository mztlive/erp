//! 身份域集合访问器；集合名由 trait 关联常量统一定义，仓储与索引共用。

use mongodb::Database;

use crate::repository::owned::{
    AccountCoreRepository, AuditEventRepository, PersonDataScopeRepository,
    PersonQueryQualificationRepository, RoleRepository,
};

/// 统一访问身份域集合，集合名供仓储和索引共用。
pub trait AccessControlExt {
    /// 人员唯一有效范围集合。
    const PERSON_DATA_SCOPES: &'static str = "person_data_scopes";
    /// `audit_event` 集合名。
    const AUDIT_EVENTS: &'static str = "audit_events";
    /// 人员查询资格集合名。
    const PERSON_QUERY_QUALIFICATIONS: &'static str = "person_query_qualifications";

    fn person_data_scopes(&self) -> PersonDataScopeRepository<'_>;
    fn accounts(&self) -> AccountCoreRepository<'_>;
    fn roles(&self) -> RoleRepository<'_>;
    fn audit_events(&self) -> AuditEventRepository<'_>;
    fn person_query_qualifications(&self) -> PersonQueryQualificationRepository<'_>;
}

impl AccessControlExt for Database {
    fn person_data_scopes(&self) -> PersonDataScopeRepository<'_> {
        PersonDataScopeRepository::new(self, Self::PERSON_DATA_SCOPES)
    }

    fn accounts(&self) -> AccountCoreRepository<'_> {
        AccountCoreRepository::new(self, "accounts")
    }

    fn roles(&self) -> RoleRepository<'_> {
        RoleRepository::new(self, "roles")
    }

    fn audit_events(&self) -> AuditEventRepository<'_> {
        AuditEventRepository::new(self, Self::AUDIT_EVENTS)
    }

    fn person_query_qualifications(&self) -> PersonQueryQualificationRepository<'_> {
        PersonQueryQualificationRepository::new(self, Self::PERSON_QUERY_QUALIFICATIONS)
    }
}
