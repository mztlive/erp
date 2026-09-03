//! 应收子账与 W13 卡券票款复核任务的原子生产。
//!
//! 任务规格、身份与摘要唯一来源为 `entities::work_item::finance_task`；
//! 本文件只解析责任人/组织、调用 factory 并持久化（FIN-E06）。

use database::{Executor, WorkItemExt};
use entities::ids::WorkItemId;
use entities::receivable::ReceivableAccount;
use entities::work_item::{
    card_funds_task_kind, new_card_funds_task, CardFundsTaskKind, CardFundsTaskSpec,
    FinanceResponsibilityOperation, WorkItem, WorkItemType, RECEIVABLE_OBJECT_TYPE,
};
use id_generator::next_id;

use crate::errors::{Error, Result};
use crate::work_item::WorkItemService;

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
    let Some((work_item_type, _)) = card_funds_task_kind(account.review_status) else {
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
    let kind = match work_item_type {
        WorkItemType::CardFundsReview => CardFundsTaskKind::Opening,
        WorkItemType::CardFundsDeltaReview => CardFundsTaskKind::Delta,
        _ => {
            return Err(Error::Internal("票款复核任务种类与待复核状态不一致".to_string()));
        }
    };
    let responsibility = WorkItemService::new(db.clone(), crate::iam::shared_rbac_service(db.clone()))
        .resolve_finance_responsibility(
            FinanceResponsibilityOperation::CardFundsReview,
            account.customer_id.as_ref(),
            executor,
        )
        .await?;
    let task = new_card_funds_task(
        WorkItemId::new(next_id()),
        CardFundsTaskSpec {
            account_id: account.base.id.clone(),
            subject_version: subject_version.to_string(),
            owner_organization_id: account.counterparty_party_id.to_string(),
            owner_user_id: responsibility.owner_user_id,
            kind,
            gross_total: account.gross_total,
            settled_total: account.settled_total,
            invoiced_total: account.invoiced_total,
        },
        responsibility.responsibility_key,
    )
    .map_err(Error::Logic)?;
    db.work_items().create(&task, executor).await?;
    Ok(Some(task))
}
