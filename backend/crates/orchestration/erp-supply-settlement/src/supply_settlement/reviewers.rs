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

/// 任务形成前仅由统一任务入口核验固定结算复核执行资格。
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
    /// 列出当前经办人可选的结算复核人；不可编辑或非本人经办的单据拒绝查询。
    ///
    /// # 参数
    /// `id` 为结算单身份，`actor` 为当前经办人；复用已注入的 RBAC 服务。
    /// # 返回
    /// 返回按姓名及账号排序的合格候选；提交仍独立重新验证资格。
    /// # 错误
    /// 非当前经办人或单据不可编辑时返回 `Forbidden`。账号或权限读取失败时返回对应错误。
    /// 岗位分离或资格不足的人员被排除，不因此失败。
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

    /// 在同一读取快照内检查结算资格并返回合格复核人。
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

/// 在调用方事务内重验岗位分离、真实账号及固定结算复核执行资格。
///
/// # 参数
/// * `authorization` - 当前统一任务资格服务。
/// * `identity_service` - 同一应用注入的共享身份与权限服务。
/// * `reviewer_id` - 复核人账号。
/// * `preparer_id` - 经办人账号。
/// * `executor` - 调用方执行器。
///
/// # 返回
/// 资格成立时无返回值。
///
/// # 错误
/// 复核人不存在时返回 `ValidationError`。账号或任务资格不满足时返回 `Forbidden`。
/// 账号或权限读取失败时返回对应错误。
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
