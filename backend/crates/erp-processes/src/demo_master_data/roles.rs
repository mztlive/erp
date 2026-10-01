//! 显式生成时准备专用演示角色，保留共享业务角色及人员范围配置。

use application_core::AuditActor;
use erp_identity::entity::rbac::PermissionSet;
use erp_identity::service::iam::{create_missing_predefined_role, predefined_role_permissions};
use erp_identity::{CreateRoleParams, Permission};

use super::spec::{AccountSpec, FoundationFile};
use super::{DemoFoundationReport, DemoMasterDataService, spec};
use crate::{Error, Result};

/// 一份明确的演示岗位授权输入，不赋予管理员或全局通配权限。
pub(super) struct DemoRole {
    pub(super) id: String,
    request: CreateRoleParams,
}

impl DemoRole {
    /// 同时绑定内建岗位身份和专用演示权限，保留人员目录资格授予来源。
    /// # 参数
    /// `account` 为当前岗位规格。
    /// # 返回
    /// 内建岗位与专用演示角色 ID。
    /// # 错误
    /// 无；角色输入已由构造过程校验。
    pub(super) fn account_role_ids(&self, account: &AccountSpec) -> Vec<String> {
        vec![account.role_id.clone(), self.id.clone()]
    }

    /// 合并当前岗位推荐权限和演示必要动作，审批人同时取得读取与决定资格。
    /// # 参数
    /// `foundation` 为基础规格，`account` 为其中一个岗位。
    /// # 返回
    /// 固定身份的专用演示角色请求。
    /// # 错误
    /// 未登记岗位或必要权限声明缺失时拒绝。
    pub(super) fn for_account(foundation: &FoundationFile, account: &AccountSpec) -> Result<Self> {
        let mut permissions = predefined_role_permissions(&account.role_id)?;
        let required = foundation
            .required_permissions
            .get(&account.key)
            .ok_or_else(|| Error::ValidationError(format!("未声明演示岗位 {} 的必要权限", account.key)))?;
        for permission in required {
            permissions.push(Permission::parse(permission)?);
        }
        if foundation.approvals.iter().flat_map(|flow| &flow.nodes).any(|node| node.assignee == account.key) {
            permissions.push(Permission::parse("approval_instance:read")?);
            permissions.push(Permission::parse("approval_instance:decide")?);
        }
        Ok(Self {
            id: format!("role-demo-{}", account.account),
            request: CreateRoleParams {
                name: format!("演示岗位-{}", account.account),
                permissions: PermissionSet::new(permissions).into_vec(),
            },
        })
    }
}

impl DemoMasterDataService {
    /// 以当前管理员授权准备缺失岗位角色和演示角色，由账号步骤绑定并继续复验资格。
    /// # 参数
    /// `actor` 为请求发起人；`report` 接收变更说明。
    /// # 返回
    /// 所有演示岗位角色均已准备，账号创建步骤负责绑定。
    /// # 错误
    /// 身份冲突、角色失效、越权或策略写入失败时停止，已完成步骤可重试。
    pub(super) async fn ensure_demo_roles(
        &self,
        actor: &AuditActor,
        report: &mut DemoFoundationReport,
    ) -> Result<()> {
        let foundation = spec::foundation_spec();
        for account in &foundation.accounts {
            create_missing_predefined_role(&self.rbac, &account.role_id, actor).await?;
            let role = DemoRole::for_account(foundation, account)?;
            let changed = self.rbac.ensure_seeded_role(&role.id, role.request, actor.clone()).await?;
            if changed {
                report.notices.push(format!("已补齐演示账号 {} 的专用岗位权限", account.account));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    /// 运行真实岗位输入构造，覆盖新目录权限、整账及同角色审批权限。
    #[test]
    fn all_demo_roles_cover_current_templates_and_required_actions() {
        let foundation = spec::foundation_spec();
        let mut ids = HashSet::new();
        for account in &foundation.accounts {
            let role = DemoRole::for_account(foundation, account).unwrap();
            assert!(ids.insert(role.id.clone()));
            assert_ne!(role.id, account.role_id);
            assert_eq!(role.account_role_ids(account), [account.role_id.clone(), role.id.clone()]);
            let permissions = PermissionSet::new(role.request.permissions);
            let baseline = PermissionSet::new(predefined_role_permissions(&account.role_id).unwrap());
            assert!(permissions.covers(&baseline));
            for required in &foundation.required_permissions[&account.key] {
                assert!(permissions.covers_one(&Permission::parse(required).unwrap()), "{required}");
            }
            assert!(!permissions.covers_one(&Permission::parse("role:create").unwrap()));
            if foundation
                .approvals
                .iter()
                .flat_map(|flow| &flow.nodes)
                .any(|node| node.assignee == account.key)
            {
                for action in ["read", "decide"] {
                    assert!(
                        permissions
                            .covers_one(&Permission::parse(format!("approval_instance:{action}")).unwrap())
                    );
                }
            }
        }
        let sales = DemoRole::for_account(foundation, &foundation.accounts[0]).unwrap();
        assert!(sales.request.permissions.contains(&Permission::parse("sales_person:list").unwrap()));
    }

    /// 未登记岗位及缺失声明在任何角色写入前拒绝。
    #[test]
    fn unknown_template_and_missing_permissions_are_rejected() {
        let mut foundation: FoundationFile =
            serde_json::from_str(include_str!("dev-foundation.json")).unwrap();
        foundation.accounts[0].role_id = "role-root".into();
        assert!(DemoRole::for_account(&foundation, &foundation.accounts[0]).is_err());
        foundation.accounts[0].role_id = "role-sales".into();
        foundation.required_permissions.remove("sales");
        assert!(DemoRole::for_account(&foundation, &foundation.accounts[0]).is_err());
    }
}
