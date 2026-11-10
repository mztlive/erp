//! 指定结算复核人的启用账号、完整财务角色资格及岗位分离。
use std::result::Result as StdResult;

use application_core::AuditActor;
use erp_core::AccountKind;
use erp_identity::{AccountSummary, Permission, PermissionSet, SharedRbacService};
use erp_workflow::ports::WorkflowAuthorizationPort;
use persistence_core::{Executor, NoTransaction};
use serde::Serialize;

use super::{SETTLEMENT_REVIEW_OWNER_ROLE, SupplierSettlementProcess};
use crate::{Error, Result};

/// 可执行当前结算复核的人员，协议只暴露选择所需信息。
#[derive(Debug, Serialize)]
pub struct SettlementReviewerOption {
    /// 选中后提交的账号身份。
    pub user_id: String,
    /// 用于人员选择的姓名。
    pub display_name: String,
    /// 用于区分同名人员的登录账号。
    pub account: String,
}

/// 判断账号是否满足结算复核的岗位、权限与责任范围要求。
fn eligible(
    account: &AccountSummary,
    preparer_id: &str,
    permissions: &PermissionSet,
    scopes: &[(String, Option<String>)],
) -> bool {
    account.id != preparer_id
        && account.is_active_backoffice()
        && [
            "supplier_settlement_statement:detail",
            "supplier_settlement_statement:confirm",
            "work_item:detail",
        ]
        .iter()
        .all(|required| permissions.covers_one(&Permission::parse(required).expect("固定复核权限合法")))
        && scopes.iter().any(|(role, _)| role == SETTLEMENT_REVIEW_OWNER_ROLE)
}

/// 候选资格由同一启用财务角色提供完整动作；对象访问由指定复核任务证明。
async fn reviewer_scopes(
    identity_service: &SharedRbacService,
    account: &AccountSummary,
    preparer: &str,
    org: &str,
    executor: &mut dyn Executor,
) -> Result<Vec<(String, Option<String>)>> {
    let _ = (preparer, org);
    let required =
        ["supplier_settlement_statement:detail", "supplier_settlement_statement:confirm", "work_item:detail"];
    let required = required.into_iter().map(Permission::parse).collect::<StdResult<Vec<_>, _>>()?;
    let snapshot = identity_service.role_permission_snapshot(account.kind, &account.id, &required).await?;
    identity_service.ensure_policy_snapshot_with_executor(snapshot.policy_revision(), executor).await?;
    let roles =
        identity_service.enabled_role_ids(&snapshot.granting_role_ids_for_all(&required), executor).await?;
    Ok(roles.into_iter().map(|role| (role.to_string(), None)).collect())
}

impl<A: WorkflowAuthorizationPort> SupplierSettlementProcess<A> {
    /// 列出当前经办人可选的结算复核人；不可编辑或非本人经办的单据拒绝查询。
    ///
    /// # 参数
    /// `id` 为结算单身份，`actor` 为当前经办人；复用已注入的 RBAC 服务。
    /// # 返回
    /// 返回按姓名及账号排序的合格候选；提交仍独立重新验证资格。
    /// # 错误
    /// 非当前经办人或单据不可编辑时返回 `Forbidden`。账号、权限或财务角色范围读取失败时返回对应错误。
    /// 岗位分离或资格不足的人员被排除，不因此失败。
    pub async fn reviewer_options(
        &self,
        id: &str,
        actor: &AuditActor,
    ) -> Result<Vec<SettlementReviewerOption>> {
        let statement =
            self.domain().access().require_statement(actor, "submit", id, &mut NoTransaction).await?;
        if !statement.is_prepared_by(actor.id()) || !statement.is_editable() {
            return Err(Error::Forbidden("仅当前经办人可为待提交的结算单选择复核人".into()));
        }
        let accounts = self
            .work_items
            .identity
            .directory()
            .accounts_by_kind(AccountKind::Admin, &mut NoTransaction)
            .await?;
        let mut options = Vec::new();
        for account in accounts {
            if account.id == actor.id() || !account.is_active_backoffice() {
                continue;
            }
            let permissions = self.work_items.identity.permissions(account.kind, &account.id).await?;
            let scopes = reviewer_scopes(
                &self.work_items.identity,
                &account,
                &statement.prepared_by,
                &statement.business_org_unit_id,
                &mut NoTransaction,
            )
            .await?;
            if eligible(&account, actor.id(), &permissions, &scopes) {
                options.push(SettlementReviewerOption {
                    user_id: account.id,
                    display_name: account.display_name,
                    account: account.login_account,
                });
            }
        }
        options.sort_by(|a, b| a.display_name.cmp(&b.display_name).then_with(|| a.account.cmp(&b.account)));
        Ok(options)
    }

    /// 在提交时重新验证人员资格并返回一致的授权版本；过期策略由提交事务拒绝。
    ///
    /// # 参数
    /// * `reviewer_id` - 拟指定的复核人。
    /// * `preparer_id` - 当前经办人，须与复核人岗位分离。
    /// * `org` - 结算单业务组织。
    ///
    /// # 返回
    /// 返回验证前后未变化的政策版本。
    ///
    /// # 错误
    /// 资格校验失败时返回对应错误。连续三次读到的政策版本不一致时返回 `ConflictError`。
    pub(super) async fn authorize_reviewer(
        &self,
        reviewer_id: &str,
        preparer_id: &str,
        org: &str,
    ) -> Result<u64> {
        for _ in 0..3 {
            let revision = self.work_items.identity.current_policy_revision().await?;
            ensure_reviewer(&self.work_items.identity, reviewer_id, preparer_id, org, &mut NoTransaction)
                .await?;
            if revision == self.work_items.identity.current_policy_revision().await? {
                return Ok(revision);
            }
        }
        Err(Error::ConflictError("复核人权限已变化，请刷新后重试".into()))
    }
}

/// 在调用方事务内重验岗位分离、账号及同一启用财务角色的完整动作。
///
/// # 参数
/// * `identity_service` - 同一应用注入的共享身份与权限服务。
/// * `reviewer_id` - 复核人账号。
/// * `preparer_id` - 经办人账号。
/// * `org` - 结算单业务组织。
/// * `executor` - 调用方执行器。
///
/// # 返回
/// 资格成立时无返回值。
///
/// # 错误
/// 复核人不存在时返回 `ValidationError`。资格、权限或财务角色不满足时返回 `Forbidden`。
/// 账号或范围读取失败时返回对应错误。
pub(super) async fn ensure_reviewer(
    identity_service: &SharedRbacService,
    reviewer_id: &str,
    preparer_id: &str,
    org: &str,
    executor: &mut dyn Executor,
) -> Result<()> {
    let account = identity_service
        .directory()
        .account(reviewer_id, executor)
        .await?
        .ok_or_else(|| Error::ValidationError("复核人不存在，请重新选择".into()))?;
    let permissions = identity_service.permissions(account.kind, reviewer_id).await?;
    let scopes = reviewer_scopes(identity_service, &account, preparer_id, org, executor).await?;
    if !eligible(&account, preparer_id, &permissions, &scopes) {
        return Err(Error::Forbidden("所选人员不能复核本单，请重新选择".into()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    /// 岗位分离、停用账号、权限不完整及错误角色范围均不进入候选。
    #[test]
    fn reviewer_requires_available_separate_account_permissions_and_finance_scope() {
        let account = AccountSummary::new("reviewer", AccountKind::Admin, true);
        let permissions = PermissionSet::new(
            ["supplier_settlement_statement:*", "work_item:detail"]
                .map(|code| Permission::parse(code).unwrap()),
        );
        let scopes = vec![(SETTLEMENT_REVIEW_OWNER_ROLE.into(), None)];
        assert!(eligible(&account, "preparer", &permissions, &scopes));
        assert!(eligible(
            &account,
            "preparer",
            &permissions,
            &[(SETTLEMENT_REVIEW_OWNER_ROLE.into(), Some("team".into()))]
        ));
        assert!(!eligible(&account, "reviewer", &permissions, &scopes));
        assert!(!eligible(
            &AccountSummary::new("reviewer", AccountKind::Admin, false),
            "preparer",
            &permissions,
            &scopes
        ));
        assert!(!eligible(
            &account,
            "preparer",
            &PermissionSet::new([Permission::parse("supplier_settlement_statement:detail").unwrap()]),
            &scopes
        ));
        assert!(!eligible(&account, "preparer", &permissions, &[("role-other".into(), None)]));
    }
}
