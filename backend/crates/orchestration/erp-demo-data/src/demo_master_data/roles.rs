//! 显式生成时准备专用演示角色，保留共享业务角色及人员范围配置。

use application_core::AuditActor;
use erp_identity::entity::role_template::BuiltinRoleState;
use erp_identity::service::iam::builtin_role_templates;
use erp_identity::{CreateRoleParams, GenerateBuiltinRolesRequest, PermissionSet};
use erp_permissions::{Permission, approval_instance as approval_instance_permissions};
#[cfg(test)]
use erp_permissions::{
    access_control as access_control_permissions, admin as admin_permissions,
    purchase_payment_request as purchase_payment_request_permissions,
};
use erp_workflow::ports::WorkflowAuthorizationPort;

use super::spec::{AccountSpec, FoundationFile};
use super::{DemoFoundationReport, DemoMasterDataProcess, spec};
use crate::{Error, Result};

/// 一份明确的演示岗位授权输入，不赋予管理员或全局通配权限。
pub(super) struct DemoRole {
    pub(super) id: String,
    request: CreateRoleParams,
}

impl DemoRole {
    // 为演示账号同时选出内建岗位和专用演示角色，使它既保留人员目录授予来源，又拥有本场景的业务动作权限。
    pub(super) fn account_role_ids(&self, account: &AccountSpec) -> Vec<String> {
        vec![account.role_id.clone(), self.id.clone()]
    }

    // 合并岗位模板和演示必要权限，确保审批人由同一角色取得读取与决定资格。未登记岗位或缺少必要权限声明时不生成角色请求。
    pub(super) fn for_account(foundation: &FoundationFile, account: &AccountSpec) -> Result<Self> {
        let mut permissions = builtin_role_templates()?
            .into_iter()
            .find(|template| template.id == account.role_id)
            .ok_or_else(|| Error::ValidationError(format!("未登记的演示岗位模板：{}", account.role_id)))?
            .permissions;
        let required = foundation
            .required_permissions
            .get(&account.key)
            .ok_or_else(|| Error::ValidationError(format!("未声明演示岗位 {} 的必要权限", account.key)))?;
        for permission in required {
            permissions.push(Permission::parse(permission)?);
        }

        if foundation.approvals.iter().flat_map(|flow| &flow.nodes).any(|node| node.assignee == account.key) {
            permissions.push(approval_instance_permissions::INSTANCE_LIST);
            permissions.push(approval_instance_permissions::SUBMIT_DECISION);
        }

        Ok(Self {
            id: format!("role-demo-{}", account.account),
            request: CreateRoleParams {
                workbenches: Default::default(),
                name: format!("演示岗位-{}", account.account),
                permissions: PermissionSet::new(permissions).into_vec(),
            },
        })
    }
}

impl<A: WorkflowAuthorizationPort> DemoMasterDataProcess<A> {
    // 用当前管理员权限准备缺失岗位及演示专用角色，供账号步骤绑定并复验。身份冲突、角色失效、越权或写入失败时停止，已完成步骤可重试。
    pub(super) async fn ensure_demo_roles(
        &self,
        actor: &AuditActor,
        report: &mut DemoFoundationReport,
    ) -> Result<()> {
        let foundation = spec::foundation_spec();
        self.ensure_builtin_roles(foundation, actor).await?;

        for account in &foundation.accounts {
            let role = DemoRole::for_account(foundation, account)?;
            let changed = self
                .identity
                .ensure_seeded_role(&role.id, role.request, actor.clone(), self.authorization.clone())
                .await?;
            if changed {
                report.notices.push(format!("已补齐演示账号 {} 的专用岗位权限", account.account));
            }
        }
        Ok(())
    }

    async fn ensure_builtin_roles(&self, foundation: &FoundationFile, actor: &AuditActor) -> Result<()> {
        let catalog = self.identity.builtin_role_catalog(actor, self.authorization.clone()).await?;
        let missing = catalog
            .templates
            .into_iter()
            .filter(|template| {
                template.state == BuiltinRoleState::Missing
                    && foundation.accounts.iter().any(|account| account.role_id == template.template.id)
            })
            .map(|template| template.template.id)
            .collect::<Vec<_>>();

        if !missing.is_empty() {
            self.identity
                .generate_builtin_roles(
                    GenerateBuiltinRolesRequest {
                        template_ids: missing,
                        expected_policy_version: catalog.policy_version,
                    },
                    actor.clone(),
                    self.authorization.clone(),
                )
                .await?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    // 验证部署使用的岗位模板已授予演示与财务执行所需动作，防止模板变更后演示流程失去资格。
    #[test]
    fn builtin_templates_cover_demo_and_finance_execution_contracts() {
        let catalog = builtin_role_templates().unwrap();
        let foundation = spec::foundation_spec();
        for account in &foundation.accounts {
            let template = catalog.iter().find(|template| template.id == account.role_id).unwrap();
            let permissions = PermissionSet::new(template.permissions.clone());
            for required in &foundation.required_permissions[&account.key] {
                assert!(
                    permissions.covers_one(&Permission::parse(required).unwrap()),
                    "{}: {required}",
                    account.key
                );
            }
        }
    }

    // 用正式角色请求构造验证全部演示岗位覆盖目录、对账和同角色审批权限，避免靠测试专用权限通过。
    #[test]
    fn all_demo_roles_cover_current_templates_and_required_actions() {
        let foundation = spec::foundation_spec();
        let catalog = builtin_role_templates().unwrap();
        let mut ids = HashSet::new();
        for account in &foundation.accounts {
            let role = DemoRole::for_account(foundation, account).unwrap();
            assert!(ids.insert(role.id.clone()));
            assert_ne!(role.id, account.role_id);
            assert_eq!(role.account_role_ids(account), [account.role_id.clone(), role.id.clone()]);

            let permissions = PermissionSet::new(role.request.permissions);
            let baseline = PermissionSet::new(
                catalog.iter().find(|template| template.id == account.role_id).unwrap().permissions.clone(),
            );
            assert!(permissions.covers(&baseline));
            for required in &foundation.required_permissions[&account.key] {
                assert!(permissions.covers_one(&Permission::parse(required).unwrap()), "{required}");
            }
            assert!(!permissions.covers_one(&admin_permissions::ROLE_CREATE_ROLE));

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
        assert!(
            sales
                .request
                .permissions
                .contains(&access_control_permissions::PERSON_DIRECTORY_LIST_SALESPEOPLE)
        );
    }

    // 验证演示角色沿用部署的细分财务模板，不能把财务复核、付款和开票写权限合并给同一岗位。
    #[test]
    fn demo_finance_positions_keep_approval_and_execution_separate() {
        let foundation = spec::foundation_spec();
        let permissions = |key: &str| {
            let account = foundation.accounts.iter().find(|account| account.key == key).unwrap();
            PermissionSet::new(DemoRole::for_account(foundation, account).unwrap().request.permissions)
        };
        let finance = permissions("finance");
        let cashier = permissions("payment");
        let invoice = permissions("invoice");
        let approve = approval_instance_permissions::SUBMIT_DECISION;
        let pay = purchase_payment_request_permissions::CREDIT_LIST;

        assert!(finance.covers_one(&approve));
        assert!(!finance.covers_one(&pay));
        assert!(cashier.covers_one(&pay));
        assert!(!cashier.covers_one(&approve));
        assert!(!invoice.covers_one(&pay));
        assert!(!invoice.covers_one(&approve));
    }

    // 验证岗位未登记或缺少声明权限时，角色请求在任何写入前就被拒绝。
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
