use super::AccessRule;
use crate::entity::access_control::ResolvedScope;
use crate::entity::resource_access::ResourceKind;
use crate::{Error, Permission, Result};

/// 每个范围节点明确登记读取补充；默认不接受任何补充。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ReadAccess {
    pub history: bool,
    pub records: bool,
}

impl ReadAccess {
    /// 检查登记是否超出该资源动作允许的读取能力。
    pub(crate) fn validate(self, permission: &Permission, history_allowed: bool) -> Result<()> {
        if (self.history || self.records) && !matches!(permission.action(), "list" | "detail") {
            return Err(Error::ValidationError("写操作不能登记读取补充".into()));
        }
        if self.history && !history_allowed {
            return Err(Error::ValidationError("该动作不接受历史参与读取".into()));
        }
        self.resource(permission).map(|_| ())
    }

    /// 只有显式开启记录授权的范围才查询授权仓储。
    pub(crate) fn resource(self, permission: &Permission) -> Result<Option<ResourceKind>> {
        if !self.records {
            return Ok(None);
        }
        match permission.resource() {
            "sales_order" => Ok(Some(ResourceKind::SalesOrder)),
            "purchase_order" => Ok(Some(ResourceKind::PurchaseOrder)),
            _ => Err(Error::ValidationError("该资源不支持指定记录读取授权".into())),
        }
    }

    /// 历史参与与个人上限的组合保持统一语义。
    pub(crate) fn base(self, scope: &ResolvedScope) -> AccessRule {
        AccessRule::scope(scope, self.history)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::access_control::{ScopeClause, ScopedObject};

    #[test]
    fn supplements_are_explicit_and_do_not_expand_write_or_unsupported_actions() {
        let permission = Permission::parse("sales_order:detail").unwrap();
        let off = ReadAccess::default();
        assert_eq!(off.resource(&permission).unwrap(), None);
        let enabled = ReadAccess { history: true, records: true };
        assert_eq!(enabled.resource(&permission).unwrap(), Some(ResourceKind::SalesOrder));
        assert!(enabled.validate(&permission, true).is_ok());
        assert!(enabled.validate(&permission, false).is_err());
        assert!(enabled.validate(&Permission::parse("sales_order:update").unwrap(), true).is_err());
        assert!(enabled.validate(&Permission::parse("customer:detail").unwrap(), true).is_err());
        let object = ScopedObject {
            owned: false,
            collaborating: false,
            historical_read_participant: true,
            org_unit_id: None,
            settlement_party_id: None,
            warehouse_id: None,
        };
        let mut scope = ResolvedScope { role_clauses: vec![], user_limit: None };
        assert!(!off.base(&scope).allows_scope(&object));
        assert!(enabled.base(&scope).allows_scope(&object));
        scope.user_limit = Some(ScopeClause::default());
        assert!(!enabled.base(&scope).allows_scope(&object));
    }
}
