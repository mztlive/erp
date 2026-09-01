// ---------------------------------------------------------------------------
// 聚合构造与校验（纯内存，不依赖仓储）
// ---------------------------------------------------------------------------

use std::str::FromStr;

use entities::common::time::{BusinessDate, Instant};
use entities::ids::ReceivableEntryId;
use entities::money::Amount;
use entities::receivable::{AccountReviewStatus, ReceivableAccountUpdate};
use entities::sales_order::{
    BusinessType, FormalRevisionContext, FormalRevisionIdentities, FormalRevisionLineIdentity,
    FormalRevisionSubtypeIdentity, RevisionSource, SalesOrder, SalesOrderRevisionAggregate,
};
use entities::sales_review::{SalesChangeSubmission, SalesChangeSubmissionLine};
use id_generator::next_id;

use crate::errors::{Error, Result};

/// 销售版本聚合载体（版本头 + 公共行 + 子类型行）。
pub(super) type RevisionAggregate = SalesOrderRevisionAggregate;

/// 汇总版本聚合的含税金额（取公共行合计；与版本头金额一致由实体保证）。
///
/// # 参数
/// * `revision` - 版本聚合
///
/// # 返回
/// 返回含税合计。
///
/// # 错误
/// 无行时返回 `ValidationError`。
fn revision_gross(revision: &RevisionAggregate) -> Result<Amount> {
    let zero = Amount::from_str("0.00").expect("静态零值必须合法");
    Ok(revision
        .lines
        .iter()
        .fold(zero, |acc, line| acc.checked_add(line.gross_amount)))
}

/// 从变更提交构建正式版本聚合（§8.1.3 变更生效）。
///
/// # 参数
/// * `order` - 销售单
/// * `submission` - 变更提交
/// * `submission_lines` - 变更提交行
/// * `revision_no` - 目标版本号（既有最大版本号 + 1）
/// * `effective_at` - 生效时间
///
/// # 返回
/// 返回版本聚合。
///
/// # 错误
/// 行字段组缺失或版本字段校验失败时返回错误。
pub(super) fn build_change_revision(
    order: &SalesOrder,
    submission: &SalesChangeSubmission,
    submission_lines: &[SalesChangeSubmissionLine],
    revision_no: u32,
    effective_at: Instant,
) -> Result<RevisionAggregate> {
    SalesOrderRevisionAggregate::from_sales_change_submission(
        allocate_formal_revision_identities(submission_lines),
        FormalRevisionContext::new(
            revision_no,
            RevisionSource::SalesChange,
            order.stable.current_revision_id.clone().map(Into::into),
            order.business_type,
            effective_at,
        ),
        submission,
        submission_lines,
    )
    .map_err(Error::Logic)
}

/// 为变更提交行分配正式版本头、公共行和子类型身份。
///
/// # 参数
/// * `lines` - 已冻结变更提交行
///
/// # 返回
/// 返回与行顺序一致的身份清单。
///
/// # 错误
/// 无。
///
/// # 关键业务约束
/// ID 由服务层生成，工厂不得调用 ID 生成器。
fn allocate_formal_revision_identities(lines: &[SalesChangeSubmissionLine]) -> FormalRevisionIdentities {
    FormalRevisionIdentities::new(
        entities::ids::SalesOrderRevisionId::new(next_id()),
        lines
            .iter()
            .map(|line| {
                FormalRevisionLineIdentity::new(
                    entities::ids::SalesOrderRevisionLineId::new(next_id()),
                    FormalRevisionSubtypeIdentity::from_line_type(line.line_type.into(), next_id()),
                )
            })
            .collect(),
    )
}

/// 构建应收差额分录（§8.1.3：新版本金额减当前版本金额，零差额不写）。
///
/// 差额必须挂到销售单既有应收子账（`account_seq = 1`）；子账缺失说明正式
/// 事实链已损坏，必须拒绝变更，不得补造兼容数据。
///
/// # 参数
/// * `order` - 销售单（含当前生效版本）
/// * `revision` - 新版本聚合
/// * `current_gross` - 当前生效版本含税合计（差额基准）
/// * `existing_account` - 既有应收子账（按销售单查询）
/// * `posted_at` - 入账时间
/// * `updated_by` - 生效变更执行人
///
/// # 返回
/// 返回 `(应收子账, 差额分录)`；差额为零时返回 `None`。
///
/// # 错误
/// 分录字段校验失败时返回错误。
pub(super) fn build_receivable_delta(
    order: &SalesOrder,
    revision: &RevisionAggregate,
    current_gross: Amount,
    existing_account: Option<entities::receivable::ReceivableAccount>,
    posted_at: Instant,
    updated_by: &str,
) -> Result<
    Option<(
        entities::receivable::ReceivableAccount,
        entities::receivable::ReceivableEntry,
    )>,
> {
    let new_gross = revision_gross(revision)?;
    let delta = new_gross.to_decimal() - current_gross.to_decimal();
    if delta.is_zero() {
        return Ok(None);
    }
    let mut account = existing_account
        .ok_or_else(|| Error::BusinessLogicError("销售单缺少正式应收子账，不能生效销售变更".to_string()))?;
    let mut account_update = receivable_delta_review_update(order.business_type, account.review_status)?;
    account_update.gross_total = Some(new_gross);
    account_update.invoiceable_total = Some(new_gross);
    account.update(account_update, updated_by).map_err(Error::Logic)?;
    let entry = entities::receivable::ReceivableEntry::new(
        ReceivableEntryId::new(next_id()),
        entities::receivable::ReceivableEntryData {
            receivable_account_id: account.base.id.clone().into(),
            entry_type: entities::receivable::ReceivableEntryType::SalesChangeDelta,
            direction: if delta.is_sign_positive() {
                entities::receivable::EntryDirection::Increase
            } else {
                entities::receivable::EntryDirection::Decrease
            },
            amount: Amount::from_str(&delta.abs().to_string()).expect("差额必须为正数金额"),
            due_date: BusinessDate::today(),
            source_fact_type: "SALES_CHANGE".to_string(),
            source_document_id: order.base.id.clone(),
            source_revision_id: revision.revision.base.id.clone(),
            source_sequence: 1,
            posted_at,
        },
    )
    .map_err(Error::Logic)?;
    Ok(Some((account, entry)))
}

/// 返回销售变更差额对应的复核缓存迁移。
///
/// 卡券差额只能建立在已完成期初/上次差额复核的账户上，并清除上一轮查询缓存；
/// 非卡券账户必须保持“不适用”。任何不一致都说明正式事实链未闭合，失败关闭。
fn receivable_delta_review_update(
    business_type: BusinessType,
    current_status: AccountReviewStatus,
) -> Result<ReceivableAccountUpdate> {
    match (business_type, current_status) {
        (BusinessType::Voucher, AccountReviewStatus::Reviewed) => Ok(ReceivableAccountUpdate {
            review_status: Some(AccountReviewStatus::SyncDeltaPending),
            reviewed_by: Some(String::new()),
            reviewed_at: None,
            review_evidence_reference: Some(String::new()),
            ..Default::default()
        }),
        (BusinessType::Voucher, _) => Err(Error::BusinessLogicError(
            "卡券上一轮票款复核尚未通过，不能生效新的销售变更差额".to_string(),
        )),
        (BusinessType::GoodsService, AccountReviewStatus::NotApplicable) => {
            Ok(ReceivableAccountUpdate::default())
        }
        (BusinessType::GoodsService, _) => Err(Error::BusinessLogicError(
            "非卡券应收账户的票款复核状态不合法，不能生效销售变更".to_string(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::receivable_delta_review_update;
    use entities::receivable::AccountReviewStatus;
    use entities::sales_order::BusinessType;

    #[test]
    fn voucher_delta_reopens_review_and_clears_previous_cache() {
        let update = receivable_delta_review_update(BusinessType::Voucher, AccountReviewStatus::Reviewed)
            .expect("已复核卡券允许形成差额复核");

        assert_eq!(update.review_status, Some(AccountReviewStatus::SyncDeltaPending));
        assert_eq!(update.reviewed_by.as_deref(), Some(""));
        assert!(update.reviewed_at.is_none());
        assert_eq!(update.review_evidence_reference.as_deref(), Some(""));
    }

    #[test]
    fn voucher_delta_requires_previous_review_to_be_complete() {
        assert!(
            receivable_delta_review_update(BusinessType::Voucher, AccountReviewStatus::OpeningPending,)
                .is_err()
        );
        assert!(
            receivable_delta_review_update(BusinessType::Voucher, AccountReviewStatus::SyncDeltaPending,)
                .is_err()
        );
    }

    #[test]
    fn goods_service_delta_keeps_review_not_applicable() {
        let update =
            receivable_delta_review_update(BusinessType::GoodsService, AccountReviewStatus::NotApplicable)
                .expect("实物服务应收不进入卡券复核");

        assert_eq!(update, Default::default());
        assert!(
            receivable_delta_review_update(BusinessType::GoodsService, AccountReviewStatus::Reviewed,)
                .is_err()
        );
    }
}
