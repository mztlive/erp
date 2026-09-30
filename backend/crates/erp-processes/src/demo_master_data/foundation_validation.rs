//! 在基础写入前校验完整审批政策、岗位引用及岗位分离。
use std::collections::HashSet;

use erp_core::AccountKind;
use erp_identity::repository::prelude::RoleRepositoryExt;
use erp_identity::{AccessControlExt, Permission};
use erp_workflow::service::approval::policy::{ALL_DOCUMENT_TYPES, ApprovalRequirement, policy_of};
use persistence_core::NoTransaction;

use super::accounts::PreparedAccounts;
use super::spec::{FoundationFile, document_type};
use super::{DemoMasterDataService, spec};
use crate::{Error, Result};

impl FoundationFile {
    /// 校验声明输入与当前审批政策严格对应。
    /// # 参数
    /// 无；读取当前基础规格。
    /// # 返回
    /// 岗位、部门、责任和全部必须审批类型一致时成功。
    /// # 错误
    /// 重复身份、缺失引用、错误审批类型或提交人参与审批时拒绝。
    pub(super) fn validate(&self) -> Result<()> {
        let keys = self.accounts.iter().map(|row| row.key.as_str()).collect::<HashSet<_>>();
        let logins = self.accounts.iter().map(|row| row.account.as_str()).collect::<HashSet<_>>();
        if keys.len() != self.accounts.len() || logins.len() != self.accounts.len() {
            return Err(Error::ValidationError("演示岗位键或登录名重复".into()));
        }
        self.validate_account_references(&keys, &logins)?;
        if self.required_permissions.len() != keys.len() {
            return Err(Error::ValidationError("每个演示岗位必须声明必要操作权限".into()));
        }
        for (key, permissions) in &self.required_permissions {
            if !keys.contains(key.as_str()) || permissions.is_empty() {
                return Err(Error::ValidationError(format!("演示权限岗位无效：{key}")));
            }
            for permission in permissions {
                Permission::parse(permission)?;
            }
        }
        let mut types = HashSet::new();
        for approval in &self.approvals {
            let kind = document_type(&approval.document_type)?;
            if !types.insert(kind) || policy_of(kind)?.requirement() != ApprovalRequirement::ProcessRequired {
                return Err(Error::ValidationError(format!(
                    "演示审批类型重复或不适用：{}",
                    approval.document_type
                )));
            }
            if !keys.contains(approval.submitter.as_str())
                || approval.nodes.is_empty()
                || approval
                    .nodes
                    .iter()
                    .any(|node| !keys.contains(node.assignee.as_str()) || node.assignee == approval.submitter)
            {
                return Err(Error::ValidationError(format!(
                    "演示审批岗位缺失或未分离：{}",
                    approval.document_type
                )));
            }
        }
        for kind in ALL_DOCUMENT_TYPES {
            if policy_of(kind)?.requirement() == ApprovalRequirement::ProcessRequired
                && !types.contains(&kind)
            {
                return Err(Error::ValidationError(format!("演示审批缺少类型：{}", kind.as_str())));
            }
        }
        Ok(())
    }

    /// 验证业务责任和范围声明中的账号均已定义且有唯一主属部门。
    fn validate_account_references(&self, keys: &HashSet<&str>, logins: &HashSet<&str>) -> Result<()> {
        for login in [
            &self.customer_owner_account,
            &self.supplier_maintainer_account,
            &self.product_maintainer_account,
            &self.warehouse_handler_account,
        ] {
            if !logins.contains(login.as_str()) {
                return Err(Error::ValidationError(format!("未定义演示负责人 {login}")));
            }
        }
        if !keys.contains(self.procurement_responsibility_owner.as_str())
            || self.finance_responsibilities.iter().any(|rule| !keys.contains(rule.owner.as_str()))
        {
            return Err(Error::ValidationError("演示默认责任人未定义".into()));
        }
        for login in logins {
            let count = self
                .departments
                .iter()
                .flat_map(|department| &department.accounts)
                .filter(|account| account.as_str() == *login)
                .count();
            if count != 1 {
                return Err(Error::ValidationError(format!("演示账号 {login} 必须有且只有一个部门")));
            }
        }
        let defaults = &self.person_scope_defaults;
        for login in defaults
            .self_accounts
            .iter()
            .chain(&defaults.department_accounts)
            .chain(defaults.company_resources.keys())
        {
            if !logins.contains(login.as_str()) {
                return Err(Error::ValidationError(format!("范围账号 {login} 未定义")));
            }
        }
        Ok(())
    }
}

impl DemoMasterDataService {
    /// 对当前启用角色逐项核验演示操作，不自动覆盖人工撤权。
    /// # 参数
    /// `accounts` 为已准备的真实岗位 ID。
    /// # 返回
    /// 所有声明动作均有启用角色授予时成功。
    /// # 错误
    /// 缺权时返回账号和具体权限；数据库或策略读取失败时停止。
    pub(super) async fn validate_demo_permissions(&self, accounts: &PreparedAccounts) -> Result<()> {
        for account in &spec::foundation_spec().accounts {
            let user = accounts
                .by_key
                .get(&account.key)
                .ok_or_else(|| Error::NotFound(format!("演示账号 {} 未准备", account.account)))?;
            let required = spec::foundation_spec().required_permissions[&account.key]
                .iter()
                .map(Permission::parse)
                .collect::<std::result::Result<Vec<_>, _>>()?;
            let snapshot = self.rbac.role_permission_snapshot(AccountKind::Admin, user, &required).await?;
            let enabled = self.db.roles().enabled_roles(snapshot.role_ids(), &mut NoTransaction).await?;
            for permission in required {
                let granting = snapshot.granting_role_ids(&permission);
                if !enabled.iter().any(|role| granting.contains(&role.base.id)) {
                    return Err(Error::Forbidden(format!(
                        "演示账号 {} 缺少操作权限 {permission}，请在角色配置中核定后重新生成",
                        account.account
                    )));
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 每例重新解析声明，避免修改全局规格。
    fn spec() -> FoundationFile {
        serde_json::from_str(include_str!("dev-foundation.json")).unwrap()
    }

    #[test]
    fn current_foundation_covers_every_required_policy() {
        spec().validate().unwrap();
    }

    #[test]
    fn missing_duplicate_and_no_approval_types_are_rejected() {
        let mut missing = spec();
        missing.approvals.pop();
        assert!(missing.validate().is_err());
        let mut duplicate = spec();
        duplicate.approvals[1].document_type = duplicate.approvals[0].document_type.clone();
        assert!(duplicate.validate().is_err());
        let mut forbidden = spec();
        forbidden.approvals[0].document_type = "supplier_payment".into();
        assert!(forbidden.validate().is_err());
    }

    #[test]
    fn broken_responsibility_and_submitter_collision_are_rejected() {
        let mut missing = spec();
        missing.procurement_responsibility_owner = "missing".into();
        assert!(missing.validate().is_err());
        let mut collision = spec();
        collision.approvals[0].nodes[0].assignee = collision.approvals[0].submitter.clone();
        assert!(collision.validate().is_err());
        let mut missing_department = spec();
        missing_department.departments.clear();
        assert!(missing_department.validate().is_err());
    }
}
