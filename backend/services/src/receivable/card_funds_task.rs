//! 应收子账与 W13 卡券票款复核任务的原子生产。

use database::{Executor, WorkItemExt};
use entities::ids::WorkItemId;
use entities::receivable::{AccountReviewStatus, ReceivableAccount};
use entities::work_item::{
    AssignmentSource, FinanceResponsibilityOperation, WorkItem, WorkItemData, WorkItemPriority, WorkItemType,
};
use id_generator::next_id;

use crate::errors::{Error, Result};
use crate::work_item::WorkItemService;

const CARD_FUNDS_OWNER_ROLE: &str = "role-finance";
const RECEIVABLE_OBJECT_TYPE: &str = "receivable_account";
const OPENING_REASON: &str = "CARD_FUNDS_OPENING_REVIEW";
const DELTA_REASON: &str = "CARD_FUNDS_DELTA_REVIEW";

/// 为新形成的待复核应收子账建立唯一正式 W13 任务。
///
/// 非复核状态不创建任务。责任人由客户精确规则或票款复核默认规则解析；应收
/// 子账、原始分录、票款任务与其它应收执行任务必须由调用方放在同一事务。
///
/// # 错误
/// 财务责任规则缺失/重复、负责人失效或权限不足、任务构造或仓储写入失败时
/// 失败关闭。
pub(crate) async fn ensure_initial_card_funds_review_task(
    db: &mongodb::Database,
    account: &ReceivableAccount,
    executor: &mut dyn Executor,
) -> Result<()> {
    ensure_card_funds_review_task(
        db,
        account,
        account.source_sales_order_revision_id.as_ref(),
        executor,
    )
    .await
    .map(|_| ())
}

/// 为当前销售正式版本形成待复核应收责任。
///
/// 首次生效使用账户来源版本；销售变更差额使用本次生效的新版本。调用方必须
/// 在写入应收状态与差额事实的同一事务中调用，避免形成无工作项的待复核状态。
///
/// # 错误
/// 财务责任规则缺失/重复、负责人失效或权限不足、任务构造或仓储写入失败时
/// 失败关闭。
pub(crate) async fn ensure_card_funds_review_task(
    db: &mongodb::Database,
    account: &ReceivableAccount,
    subject_version: &str,
    executor: &mut dyn Executor,
) -> Result<Option<WorkItem>> {
    let Some((work_item_type, reason_code)) = review_task_spec(account.review_status) else {
        return Ok(None);
    };
    let existing = db
        .work_items()
        .list_active_by_object(RECEIVABLE_OBJECT_TYPE, account.base.id.as_str(), executor)
        .await?
        .into_iter()
        .filter(|item| item.work_item_type == work_item_type)
        .collect::<Vec<_>>();
    if existing.len() > 1 {
        return Err(Error::Internal(
            "同一应收账户存在多个开放票款复核任务".to_string(),
        ));
    }
    if let Some(item) = existing.into_iter().next() {
        if item.subject_version != subject_version {
            return Err(Error::ConflictError(
                "开放票款复核任务绑定的销售版本与当前待复核版本不一致".to_string(),
            ));
        }
        return Ok(Some(item));
    }
    let responsibility = WorkItemService::new(db.clone(), crate::iam::shared_rbac_service(db.clone()))
        .resolve_finance_responsibility(
            FinanceResponsibilityOperation::CardFundsReview,
            account.customer_id.as_ref(),
            executor,
        )
        .await?;
    let task = WorkItem::new_with_responsibility_key(
        WorkItemId::new(next_id()),
        WorkItemData {
            work_item_type,
            business_object_type: RECEIVABLE_OBJECT_TYPE.to_string(),
            business_object_id: account.base.id.clone(),
            subject_version: subject_version.to_string(),
            owner_role: CARD_FUNDS_OWNER_ROLE.to_string(),
            owner_organization_id: account.counterparty_party_id.to_string(),
            owner_user_id: responsibility.owner_user_id,
            assignment_source: AssignmentSource::SystemRule,
            priority: WorkItemPriority::High,
            due_at: None,
            reason_code: Some(reason_code.to_string()),
            impact_summary: Some(format!(
                "应收 ¥{}，已到账 ¥{}，已开票 ¥{}；请核对票款正式事实",
                account.gross_total, account.settled_total, account.invoiced_total
            )),
        },
        responsibility.responsibility_key,
    )
    .map_err(Error::Logic)?;
    db.work_items().create(&task, executor).await?;
    Ok(Some(task))
}

fn review_task_spec(status: AccountReviewStatus) -> Option<(WorkItemType, &'static str)> {
    match status {
        AccountReviewStatus::OpeningPending => Some((WorkItemType::CardFundsReview, OPENING_REASON)),
        AccountReviewStatus::SyncDeltaPending => Some((WorkItemType::CardFundsDeltaReview, DELTA_REASON)),
        AccountReviewStatus::NotApplicable | AccountReviewStatus::Reviewed => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{review_task_spec, DELTA_REASON, OPENING_REASON};
    use entities::receivable::AccountReviewStatus;
    use entities::work_item::WorkItemType;

    #[test]
    fn only_pending_review_states_create_w13_tasks() {
        assert_eq!(
            review_task_spec(AccountReviewStatus::OpeningPending),
            Some((WorkItemType::CardFundsReview, OPENING_REASON))
        );
        assert_eq!(
            review_task_spec(AccountReviewStatus::SyncDeltaPending),
            Some((WorkItemType::CardFundsDeltaReview, DELTA_REASON))
        );
        assert!(review_task_spec(AccountReviewStatus::NotApplicable).is_none());
        assert!(review_task_spec(AccountReviewStatus::Reviewed).is_none());
    }
}
