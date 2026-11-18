//! 指定结算复核人的启用账号、固定任务执行资格及岗位分离。

use application_core::AuditActor;
use erp_core::AccountKind;
use erp_identity::entity::authorization::{FactValue, ObjectFacts};
use erp_identity::{AccessTarget, AuthorizationService, Error as IdentityError, SharedRbacService};
use erp_permissions::internal;
use erp_supply::service::supplier_settlement::SettlementAction;
use erp_workflow::ports::WorkflowAuthorizationPort;
use persistence_core::{Executor, Transactional};
use serde::Serialize;

use super::SupplierSettlementProcess;
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

// 任务建立前通过统一任务授权入口查询固定结算复核资格，供候选筛选和正式复验使用。
async fn qualified_reviewer(
    authorization: &AuthorizationService,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<bool> {
    let candidate = ObjectFacts {
        resource: "work_item".into(),
        subject_type: FactValue::Known("SUPPLIER_SETTLEMENT_REVIEW".into()),
        subject_source_resource: FactValue::Known("supplier_settlement_statement".into()),
        ..Default::default()
    };
    match authorization
        .require(
            actor,
            &internal::WORK_ITEM_EXECUTION_QUALIFICATION,
            AccessTarget::Candidate(candidate),
            executor,
        )
        .await
    {
        Ok(()) => Ok(true),
        Err(IdentityError::Forbidden(_)) => Ok(false),
        Err(error) => Err(error.into()),
    }
}

impl<A: WorkflowAuthorizationPort> SupplierSettlementProcess<A> {
    /// 为可编辑且由本人经办的结算单列出财务复核候选，按姓名和账号排序；排除不满足资格或岗位分离的人员。
    ///
    /// 非本人经办或不可编辑时拒绝，读取失败直接返回；提交复核时仍需重新校验所选人员。
    pub async fn reviewer_options(
        &self,
        id: &str,
        actor: &AuditActor,
    ) -> Result<Vec<SettlementReviewerOption>> {
        let process = self.clone();
        let id = id.to_owned();
        let actor = actor.clone();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move { process.reviewer_options_apply(&id, &actor, executor).await })
            })
            .await
    }

    // 在同一读取快照确认经办人对结算单的资格并筛选可执行复核的人员，避免候选基于不同版本的事实。
    async fn reviewer_options_apply(
        &self,
        id: &str,
        actor: &AuditActor,

        executor: &mut dyn Executor,
    ) -> Result<Vec<SettlementReviewerOption>> {
        self.domain().access().require_action(actor, SettlementAction::Submit, id, executor).await?;
        let statement = self.domain().load_statement(id, executor).await?;
        if !statement.is_editable() {
            return Err(Error::Forbidden("仅当前经办人可为待提交的结算单选择复核人".into()));
        }

        let accounts =
            self.work_items.identity.directory().accounts_by_kind(AccountKind::Admin, executor).await?;
        let mut options = Vec::new();
        for account in accounts {
            if account.id == actor.id() {
                continue;
            }
            let candidate = AuditActor::new(account.id.clone(), account.login_account.clone(), account.kind);
            if qualified_reviewer(&self.authorization, &candidate, executor).await? {
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
}

// 用原事务核验所选复核人是真实账号、并非制单人且具有固定任务执行资格，供提交和决定前把关。
// 账号不存在返回 `ValidationError`，岗位或任务资格不符返回 `Forbidden`，读取错误原样向上返回。
pub(super) async fn ensure_reviewer(
    authorization: &AuthorizationService,
    identity_service: &SharedRbacService,
    reviewer_id: &str,
    preparer_id: &str,
    executor: &mut dyn Executor,
) -> Result<()> {
    let account = identity_service
        .directory()
        .account(reviewer_id, executor)
        .await?
        .ok_or_else(|| Error::ValidationError("复核人不存在，请重新选择".into()))?;
    if reviewer_id == preparer_id {
        return Err(Error::Forbidden("所选人员不能复核本单，请重新选择".into()));
    }

    let actor = AuditActor::new(account.id, account.login_account, account.kind);
    if qualified_reviewer(authorization, &actor, executor).await? {
        return Ok(());
    }
    Err(Error::Forbidden("所选人员不能复核本单，请重新选择".into()))
}
