//! 显式生成时准备专用演示角色，保留共享业务角色及人员范围配置。

use application_core::AuditActor;
use erp_core::AccountKind;
use erp_identity::entity::rbac::PermissionSet;
use erp_identity::service::iam::predefined_role_permissions;
use erp_identity::{AdminService, CreateRoleParams, Permission, UpdateAdminRoleParams};

use super::accounts::PreparedAccounts;
use super::spec::{AccountSpec, FoundationFile};
use super::{DemoFoundationReport, DemoMasterDataService, spec};
use crate::{Error, Result};

/// 一份明确的演示岗位授权输入，不赋予管理员或全局通配权限。
struct DemoRole {
    id: String,
    request: CreateRoleParams,
}

impl DemoRole {
    /// 合并当前岗位推荐权限和演示必要动作，审批人同时取得读取与决定资格。
    fn for_account(foundation: &FoundationFile, account: &AccountSpec) -> Result<Self> {
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
    /// 以当前管理员授权准备演示角色并追加绑定，然后由基础准备继续复验资格。
    /// # 参数
    /// `actor` 为请求发起人；`accounts` 为已准备账号；`report` 接收变更说明。
    /// # 返回
    /// 所有演示岗位角色均已准备并绑定。
    /// # 错误
    /// 身份冲突、角色失效、越权或策略写入失败时停止，已完成步骤可重试。
    pub(super) async fn ensure_demo_roles(
        &self,
        actor: &AuditActor,
        accounts: &PreparedAccounts,
        report: &mut DemoFoundationReport,
    ) -> Result<()> {
        let foundation = spec::foundation_spec();
        for account in &foundation.accounts {
            let user = accounts
                .by_key
                .get(&account.key)
                .ok_or_else(|| Error::NotFound(format!("演示账号 {} 未准备", account.account)))?;
            let role = DemoRole::for_account(foundation, account)?;
            let changed = self.rbac.ensure_seeded_role(&role.id, role.request, actor.clone()).await?;
            let bound = self.bind_demo_role(user, &role.id, actor).await?;
            if changed || bound {
                report.notices.push(format!("已补齐演示账号 {} 的专用岗位权限", account.account));
            }
        }
        Ok(())
    }

    /// 追加专用演示角色绑定，不替换账号已有的其他角色。
    async fn bind_demo_role(&self, user: &str, role_id: &str, actor: &AuditActor) -> Result<bool> {
        let mut role_ids = self.rbac.role_ids(AccountKind::Admin, user).await?;
        if role_ids.iter().any(|id| id == role_id) {
            return Ok(false);
        }
        role_ids.push(role_id.into());
        AdminService::new(self.db.clone(), self.rbac.clone())
            .update_admin_role(UpdateAdminRoleParams { id: user.into(), role_ids }, actor.clone())
            .await?;
        Ok(true)
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
