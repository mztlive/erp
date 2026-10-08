//! 个人业务扩展授权，与个人 DataScope 上限独立持久化。
use entity_core::BaseModel;
use entity_macros::{Entity, id_type};
use erp_core::validation::normalize_required_text_ref;
use serde::{Deserialize, Serialize};

use super::{
    DataScope, DataScopeData, DataScopeId, DataScopeSubjectType, DataScopeType, ScopeBinding, ScopeDimension,
    ScopeTargetMode,
};
use crate::entity::rbac::RoleId;
use crate::{Error, Result};

id_type!(PersonalBusinessGrantId);

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

/// 新增个人部门授权的数据，不接收系统字段。
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PersonalBusinessGrantData {
    pub role_id: String,
    pub resource: String,
    pub actions: Vec<String>,
    pub org_unit_ids: Vec<String>,
    pub include_descendants: bool,
}

impl PersonalBusinessGrant {
    /// 规范化身份、明确动作和部门目标。
    /// # 参数
    /// * `id` - 新授权身份。
    /// * `user_id` - 固定人员身份。
    /// * `data` - 角色、业务动作和部门。
    /// # 返回
    /// 校验后的独立正向授权。
    /// # 错误
    /// 空身份、未知业务、非法动作和部门目标拒绝。
    pub fn new(id: PersonalBusinessGrantId, user_id: &str, data: PersonalBusinessGrantData) -> Result<Self> {
        let mut grant = Self {
            base: BaseModel::new(id.to_string()),
            user_id: normalize_required_text_ref(user_id, "请选择人员", 128, "人员身份过长")?,
            role_id: RoleId::parse(data.role_id)?.to_string(),
            resource: data.resource,
            actions: data.actions,
            org_unit_ids: data.org_unit_ids,
            include_descendants: data.include_descendants,
        };
        grant.actions.sort();
        grant.actions.dedup();
        let rule = grant.as_role_scope()?;
        grant.org_unit_ids = rule.scope_targets;
        Ok(grant)
    }

    /// 核对撤销命令的人员边界和实体版本。
    /// # 参数
    /// * `user` - 路径中的固定人员。
    /// * `version` - 用户读取的授权版本。
    /// # 返回
    /// 当前授权可撤销时成功。
    /// # 错误
    /// 跨人员操作或版本冲突拒绝。
    pub fn ensure_revoke(&self, user: &str, version: u64) -> Result<()> {
        if self.user_id != user {
            return Err(Error::NotFound("该人员的业务授权不存在".into()));
        }
        if self.base.version != version || self.base.is_deleted() {
            return Err(Error::ConflictError("授权已变化，请刷新后重试".into()));
        }
        Ok(())
    }

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
    fn data() -> PersonalBusinessGrantData {
        PersonalBusinessGrantData {
            role_id: "sales".into(),
            resource: "sales_order".into(),
            actions: vec!["detail".into(), "detail".into()],
            org_unit_ids: vec!["one".into()],
            include_descendants: false,
        }
    }
    #[test]
    fn input_is_explicit_and_does_not_allow_other_dimensions() {
        let grant = PersonalBusinessGrant::new(PersonalBusinessGrantId::new("g"), "alice", data()).unwrap();
        assert_eq!(grant.actions, vec!["detail"]);
        assert!(grant.ensure_revoke("alice", 1).is_ok());
        assert!(grant.ensure_revoke("bob", 1).is_err());
        assert!(grant.ensure_revoke("alice", 2).is_err());
        for resource in ["warehouse", "approval_instance", "org_unit", "unknown"] {
            assert!(
                PersonalBusinessGrant::new(
                    PersonalBusinessGrantId::new("g"),
                    "alice",
                    PersonalBusinessGrantData { resource: resource.into(), ..data() }
                )
                .is_err()
            );
        }
        for actions in [vec![], vec!["*".into()]] {
            assert!(
                PersonalBusinessGrant::new(
                    PersonalBusinessGrantId::new("g"),
                    "alice",
                    PersonalBusinessGrantData { actions, ..data() }
                )
                .is_err()
            );
        }
        assert!(
            PersonalBusinessGrant::new(
                PersonalBusinessGrantId::new("g"),
                "alice",
                PersonalBusinessGrantData { org_unit_ids: vec![], ..data() }
            )
            .is_err()
        );
    }
}
