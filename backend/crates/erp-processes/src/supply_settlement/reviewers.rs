//! 结算复核人的可用账号、执行权限和财务责任范围校验。
use application_core::AuditActor;
use erp_core::AccountKind;
use erp_workflow::entity::work_item::AvailableWorkItemAccount;
use erp_workflow::ports::{WorkflowAccountFact, WorkflowAuthorizationPort, permission_covers};
use persistence_core::NoTransaction;
use serde::Serialize;

use super::{
    SETTLEMENT_REVIEW_OWNER_ORGANIZATION_ID, SETTLEMENT_REVIEW_OWNER_ROLE, SupplierSettlementProcess,
};
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
    account: &WorkflowAccountFact,
    preparer_id: &str,
    permissions: &[String],
    scopes: &[(String, Option<String>)],
) -> bool {
    account.id != preparer_id
        && AvailableWorkItemAccount::from_account_kind(account, AccountKind::Admin).is_ok()
        && [
            "supplier_settlement_statement:detail",
            "supplier_settlement_statement:confirm",
            "work_item:detail",
        ]
        .iter()
        .all(|required| permissions.iter().any(|owned| permission_covers(owned, required)))
        && scopes.iter().any(|(role, organization)| {
            role == SETTLEMENT_REVIEW_OWNER_ROLE
                && organization.as_deref().is_none_or(|id| id == SETTLEMENT_REVIEW_OWNER_ORGANIZATION_ID)
        })
}

/// 岗位资格保留原财务角色；范围由 confirm 动作及经办人的当前部门独立证明。
async fn reviewer_scopes(
    auth: &impl WorkflowAuthorizationPort,
    account: &WorkflowAccountFact,
    preparer: &str,
) -> Result<Vec<(String, Option<String>)>> {
    let actor = AuditActor::new(account.id.clone(), account.login_account.clone(), account.kind);
    let Some(scope) = auth
        .resolve_workflow_scope(&actor, "supplier_settlement_statement:confirm", &mut NoTransaction)
        .await?
    else {
        return Ok(Vec::new());
    };
    let object =
        erp_workflow::ports::WorkflowScopeObject { owner_user_id: preparer.into(), ..Default::default() };
    if !scope.allows_role(SETTLEMENT_REVIEW_OWNER_ROLE, &object) {
        return Ok(Vec::new());
    }
    Ok(scope
        .granting_role_ids
        .into_iter()
        .map(|role| (role, Some(SETTLEMENT_REVIEW_OWNER_ORGANIZATION_ID.into())))
        .collect())
}

impl SupplierSettlementProcess {
    /// 列出当前经办人可选的结算复核人；不可编辑或非本人经办的单据拒绝查询。
    pub async fn reviewer_options(
        &self,
        id: &str,
        actor: &AuditActor,
    ) -> Result<Vec<SettlementReviewerOption>> {
        let statement = self.domain().load_statement(id, &mut NoTransaction).await?;
        if !statement.is_prepared_by(actor.id()) || !statement.is_editable() {
            return Err(Error::Forbidden("仅当前经办人可为待提交的结算单选择复核人".into()));
        }
        let auth = crate::adapters::workflow::workflow_auth(
            self.db.clone(),
            crate::adapters::identity::shared_rbac_service(self.db.clone()),
        );
        let accounts = auth.list_accounts_by_kind(AccountKind::Admin, &mut NoTransaction).await?;
        let mut options = Vec::new();
        for account in accounts {
            if account.id == actor.id()
                || AvailableWorkItemAccount::from_account_kind(&account, AccountKind::Admin).is_err()
            {
                continue;
            }
            let permissions = auth.permission_codes(account.kind, &account.id).await?;
            let scopes = reviewer_scopes(&auth, &account, actor.id()).await?;
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
    pub(super) async fn authorize_reviewer(
        &self,
        auth: &impl WorkflowAuthorizationPort,
        reviewer_id: &str,
        preparer_id: &str,
    ) -> Result<u64> {
        for _ in 0..3 {
            let revision = auth.current_policy_revision().await?;
            let account = auth
                .load_account(reviewer_id, &mut NoTransaction)
                .await?
                .ok_or_else(|| Error::ValidationError("复核人不存在，请重新选择".into()))?;
            let permissions = auth.permission_codes(account.kind, reviewer_id).await?;
            let scopes = reviewer_scopes(auth, &account, preparer_id).await?;
            if !eligible(&account, preparer_id, &permissions, &scopes) {
                return Err(Error::ValidationError("所选人员不能复核本单，请重新选择".into()));
            }
            if revision == auth.current_policy_revision().await? {
                return Ok(revision);
            }
        }
        Err(Error::ConflictError("复核人权限已变化，请刷新后重试".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    /// 岗位分离、停用账号、权限不完整及错误角色范围均不进入候选。
    #[test]
    fn reviewer_requires_available_separate_account_permissions_and_finance_scope() {
        let account = WorkflowAccountFact::new("reviewer", AccountKind::Admin, true);
        let permissions = vec!["supplier_settlement_statement:*".into(), "work_item:detail".into()];
        let scopes = vec![(SETTLEMENT_REVIEW_OWNER_ROLE.into(), Some("company".into()))];
        assert!(eligible(&account, "preparer", &permissions, &scopes));
        assert!(eligible(&account, "preparer", &permissions, &[(SETTLEMENT_REVIEW_OWNER_ROLE.into(), None)]));
        assert!(!eligible(&account, "reviewer", &permissions, &scopes));
        assert!(!eligible(
            &WorkflowAccountFact::new("reviewer", AccountKind::Admin, false),
            "preparer",
            &permissions,
            &scopes
        ));
        assert!(!eligible(&account, "preparer", &["supplier_settlement_statement:detail".into()], &scopes));
        assert!(!eligible(&account, "preparer", &permissions, &[("role-other".into(), None)]));
        assert!(!eligible(
            &account,
            "preparer",
            &permissions,
            &[(SETTLEMENT_REVIEW_OWNER_ROLE.into(), Some("team".into()))]
        ));
    }
}
