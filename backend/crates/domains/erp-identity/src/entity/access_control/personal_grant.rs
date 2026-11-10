//! 旧个人业务扩展授权的迁移读取模型。
use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use super::{
    DataScope, DataScopeData, DataScopeId, DataScopeSubjectType, DataScopeType, ScopeBinding, ScopeDimension,
    ScopeTargetMode,
};
use crate::{Error, Result};

/// 同一有效角色支持的个人部门授权；不授予动作权限。
#[derive(Debug, Clone, Serialize, Deserialize, Entity)]
pub struct PersonalBusinessGrant {
    #[serde(flatten)]
    pub base: BaseModel,
    pub user_id: String,
    pub role_id: String,
    pub resource: String,
    pub actions: Vec<String>,
    pub org_unit_ids: Vec<String>,
    pub include_descendants: bool,
}

impl PersonalBusinessGrant {
    /// 转为同角色的范围证据；调用方仍须检查人员、有效角色与动作。
    /// # 参数
    /// 无。
    /// # 返回
    /// 仅内部组织维度的显式角色规则。
    /// # 错误
    /// 未支持个人部门授权的业务或非法形态拒绝。
    pub fn as_role_scope(&self) -> Result<DataScope> {
        if !department_grant_resource(&self.resource) {
            return Err(Error::ValidationError("该业务不支持按部门扩大个人范围".into()));
        }
        let mut rule = DataScope::new(
            DataScopeId::new(&self.base.id),
            DataScopeData {
                binding: ScopeBinding {
                    schema_version: 2,
                    resource: self.resource.clone(),
                    actions: self.actions.clone(),
                    target_dimension: ScopeDimension::InternalOrg,
                    target_mode: Some(ScopeTargetMode::Explicit),
                    include_descendants: Some(self.include_descendants),
                    enabled: true,
                },
                subject_type: DataScopeSubjectType::Role,
                subject_id: self.role_id.clone(),
                scope_type: DataScopeType::Organization,
                scope_targets: self.org_unit_ids.clone(),
            },
        )?;
        rule.base = self.base.clone();
        Ok(rule)
    }

    /// 只有当前人员、同一有效角色及明确资源动作匹配才贡献范围。
    /// # 参数
    /// * `user` - 操作人。
    /// * `roles` - 已证明完整权限的有效角色。
    /// * `resource`、`action` - 当前业务动作。
    /// # 返回
    /// 匹配且未撤销时为真。
    /// # 错误
    /// 不返回错误。
    pub fn applies(&self, user: &str, roles: &[String], resource: &str, action: &str) -> bool {
        !self.base.is_deleted()
            && self.user_id == user
            && roles.contains(&self.role_id)
            && self.resource == resource
            && self.actions.iter().any(|item| item == action)
    }
}

/// 已由业务对象责任/部门事实支撑的单组织维度业务。
/// # 参数
/// * `resource` - 资源代码。
/// # 返回
/// 是否可按部门扩大范围并设置本人默认范围。
/// # 错误
/// 不返回错误。
pub fn department_grant_resource(resource: &str) -> bool {
    matches!(
        resource,
        "customer"
            | "contract"
            | "sales_order"
            | "purchase_order"
            | "cost_entry"
            | "cost_allocation"
            | "sales_selection_booklet"
            | "sales_selection_proposal"
            | "receivable_account"
            | "customer_receipt"
            | "invoice"
            | "sales_invoice_request"
            | "payable_account"
            | "supplier_payment"
            | "purchase_invoice_allocation"
            | "supplier_settlement_statement"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 构造迁移读取的旧集合记录，不恢复旧创建接口。
    fn grant() -> PersonalBusinessGrant {
        PersonalBusinessGrant {
            base: BaseModel::fake(),
            user_id: "alice".into(),
            role_id: "sales".into(),
            resource: "sales_order".into(),
            actions: vec!["detail".into()],
            org_unit_ids: vec!["one".into()],
            include_descendants: false,
        }
    }

    #[test]
    fn migration_rejects_invalid_legacy_targets_and_preserves_identity() {
        let value = grant();
        let rule = value.as_role_scope().unwrap();
        assert_eq!(rule.subject_id, "sales");
        assert_eq!(rule.base, value.base);
        assert_eq!(rule.scope_targets, ["one"]);
        for resource in ["warehouse", "approval_instance", "org_unit", "unknown"] {
            assert!(PersonalBusinessGrant { resource: resource.into(), ..grant() }.as_role_scope().is_err());
        }
        for actions in [vec![], vec!["*".into()]] {
            assert!(PersonalBusinessGrant { actions, ..grant() }.as_role_scope().is_err());
        }
        assert!(PersonalBusinessGrant { org_unit_ids: vec![], ..grant() }.as_role_scope().is_err());
    }

    #[test]
    fn migration_only_applies_current_user_role_and_action_grants() {
        let mut value = grant();
        let roles = ["sales".into()];
        assert!(value.applies("alice", &roles, "sales_order", "detail"));
        assert!(!value.applies("bob", &roles, "sales_order", "detail"));
        assert!(!value.applies("alice", &[], "sales_order", "detail"));
        assert!(!value.applies("alice", &roles, "purchase_order", "detail"));
        assert!(!value.applies("alice", &roles, "sales_order", "update"));
        value.base.deleted_at = 10;
        assert!(!value.applies("alice", &roles, "sales_order", "detail"));
    }
}
