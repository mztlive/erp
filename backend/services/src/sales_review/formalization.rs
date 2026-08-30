// ---------------------------------------------------------------------------
// 聚合构造与校验（纯内存，不依赖仓储）
// ---------------------------------------------------------------------------

use std::str::FromStr;

use entities::common::time::{BusinessDate, Instant};
use entities::ids::{ReceivableEntryId, SalesOrderRevisionId, SalesOrderRevisionLineId};
use entities::money::Amount;
use entities::receivable::{AccountReviewStatus, ReceivableAccountUpdate};
use entities::sales_order::{
    BusinessType, RevisionSource, SalesOrder, SalesOrderGoodsServiceLineRevision,
    SalesOrderGoodsServiceLineRevisionData, SalesOrderGoodsServiceLineRevisionId, SalesOrderRevision,
    SalesOrderRevisionData, SalesOrderRevisionLine, SalesOrderRevisionLineData,
    SalesOrderVoucherLineRevision, SalesOrderVoucherLineRevisionData, SalesOrderVoucherLineRevisionId,
};
use entities::sales_review::{SalesChangeSubmission, SalesChangeSubmissionLine};
use id_generator::next_id;

use super::sales_change_mapping::{
    change_submission_goods, change_submission_voucher, convert_line_type_to_sales,
};
use crate::errors::{Error, Result};

/// 销售版本聚合载体（版本头 + 公共行 + 子类型行）。
pub(super) struct RevisionAggregate {
    /// 版本头实体。
    pub(super) revision: SalesOrderRevision,
    /// 公共行版本。
    pub(super) lines: Vec<SalesOrderRevisionLine>,
    /// 公共行版本。
    pub(super) goods_lines: Vec<SalesOrderGoodsServiceLineRevision>,
    /// 卡券行版本。
    pub(super) voucher_lines: Vec<SalesOrderVoucherLineRevision>,
}

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
    let revision_id = SalesOrderRevisionId::new(next_id());
    let revision = SalesOrderRevision::new(
        revision_id.clone(),
        SalesOrderRevisionData {
            sales_order_id: submission.sales_order_id.clone(),
            revision_no,
            revision_source: RevisionSource::SalesChange,
            source_snapshot_id: None,
            previous_revision_id: order.stable.current_revision_id.clone().map(Into::into),
            content_hash: format!("sub:{}", submission.base.id),
            customer_revision_id: None,
            contract_revision_id: submission.contract_revision_id.clone(),
            snapshot: entities::sales_order::HeaderSnapshotData {
                customer_name: submission.customer_snapshot.customer_name.clone(),
                contract_no: submission
                    .contract_snapshot
                    .as_ref()
                    .map(|snapshot| snapshot.contract_no.clone()),
                settlement_party_name: submission
                    .settlement_party_snapshot
                    .as_ref()
                    .map(|snapshot| snapshot.settlement_party_name.clone()),
                payment_term_code: submission.payment_term_snapshot.payment_term_code.clone(),
                payment_term_name: submission.payment_term_snapshot.payment_term_name.clone(),
                invoice_type: submission.invoice_requirement_snapshot.invoice_type.clone(),
                tax_point: submission.invoice_requirement_snapshot.tax_point.clone(),
            },
            project_name: submission.project_name.clone(),
            business_remark: submission.business_remark.clone(),
            voucher_category_sku_id: submission.voucher_category_sku_id.clone(),
            voucher_expiry_at: submission.voucher_expiry_at,
            gross_amount: submission.gross_amount,
            net_amount: submission.net_amount,
            tax_amount: submission.tax_amount,
            effective_at,
            recorded_at: effective_at,
        },
    )?;
    let mut revision_lines = Vec::with_capacity(submission_lines.len());
    let mut goods_lines = Vec::new();
    let mut voucher_lines = Vec::new();
    for sub_line in submission_lines {
        let revision_line_id = SalesOrderRevisionLineId::new(next_id());
        let revision_line = SalesOrderRevisionLine::new(
            revision_line_id.clone(),
            SalesOrderRevisionLineData {
                sales_order_revision_id: revision_id.clone(),
                sales_order_line_id: sub_line.sales_order_line_id.clone(),
                line_no: sub_line.line_no,
                line_type: convert_line_type_to_sales(sub_line.line_type),
                gross_amount: sub_line.gross_amount,
                net_amount: sub_line.net_amount,
                tax_amount: sub_line.tax_amount,
                sales_tax_rate: sub_line.sales_tax_rate,
                item_name_snapshot: sub_line.item_name_snapshot.clone(),
                spec_snapshot: sub_line.spec_snapshot.clone(),
                unit_snapshot: sub_line.unit_snapshot.clone(),
            },
        )?;
        match sub_line.line_type {
            entities::sales_review::LineType::GoodsService => {
                let goods = change_submission_goods(sub_line)?;
                goods_lines.push(SalesOrderGoodsServiceLineRevision::new(
                    SalesOrderGoodsServiceLineRevisionId::new(next_id()),
                    SalesOrderGoodsServiceLineRevisionData {
                        revision_line_id: revision_line_id.clone(),
                        sku_id: goods.sku_id,
                        sku_revision_id: goods.sku_revision_id,
                        welfare_scenario: goods.welfare_scenario,
                        service_region: goods.service_region,
                        fulfillment_due_at: goods.fulfillment_due_at,
                        quantity: goods.quantity,
                        base_unit_code: goods.base_unit_code,
                        unit_price_gross: goods.unit_price_gross,
                    },
                )?);
            }
            entities::sales_review::LineType::Voucher => {
                let voucher = change_submission_voucher(sub_line)?;
                voucher_lines.push(SalesOrderVoucherLineRevision::new(
                    SalesOrderVoucherLineRevisionId::new(next_id()),
                    SalesOrderVoucherLineRevisionData {
                        revision_line_id: revision_line_id.clone(),
                        face_value: voucher.face_value,
                        card_count: voucher.card_count,
                        unit_price_gross: voucher.unit_price_gross,
                        card_form: voucher.card_form,
                    },
                )?);
            }
        }
        revision_lines.push(revision_line);
    }
    Ok(RevisionAggregate {
        revision,
        lines: revision_lines,
        goods_lines,
        voucher_lines,
    })
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
