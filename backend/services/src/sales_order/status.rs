//! 销售单阶段判定：阶段 `(code, label, tone)`、结案资格、销售变更发起资格。

use entities::money::Amount;
use entities::sales_order::{
    BusinessType, CloseStatus, CollectionProgress, CommercialStatus, FulfillmentProgress, InvoiceProgress,
    OriginSystem, ReviewStatus,
};

use super::dto;

/// 按“有效草稿负责人 → 最新提交人 → 建单人”确定销售单详情负责人。
pub(super) fn detail_owner_user_id(
    working_copy_editor: Option<&str>,
    latest_submitter: Option<&str>,
    created_by: &str,
) -> String {
    working_copy_editor
        .or(latest_submitter)
        .unwrap_or(created_by)
        .to_string()
}

/// 销售单当前阶段 `(code, label, tone)`（服务端权威计算）。
///
/// 移植自 erp-client `api.ts::mapPrimaryStatus`；用真实枚举类型消掉了原实现里
/// 防御性的字符串兜底分支——后端只会产出这里覆盖到的枚举组合（`Draft` 恒配
/// `NotSubmitted`，`PendingReview` 恒配非 `NotSubmitted` 的审核轨阶段，见
/// `SalesOrder::submit_for_review`/`return_to_draft` 的不变式，两者不会交叉）。
pub(super) fn stage_code_label_tone(
    commercial: CommercialStatus,
    review: ReviewStatus,
    close: CloseStatus,
    fulfillment: FulfillmentProgress,
) -> (&'static str, &'static str, &'static str) {
    if close == CloseStatus::Closed {
        return ("closed", "已关闭", "void");
    }
    if commercial == CommercialStatus::Voided {
        return ("voided", "已作废", "void");
    }
    if commercial == CommercialStatus::Effective {
        return if fulfillment == FulfillmentProgress::PartiallyFulfilled {
            ("fulfilling", "履约中", "info")
        } else {
            ("effective", "已生效", "success")
        };
    }
    match review {
        ReviewStatus::PendingProcurementConfirmation => ("awaiting_confirm", "待二次确认", "warning"),
        ReviewStatus::PendingLowMarginSuperior | ReviewStatus::Rejected => {
            ("awaiting_sales", "待销售处理", "warning")
        }
        ReviewStatus::PendingSalesLeader => ("awaiting_sales_lead", "待销售领导审批", "warning"),
        ReviewStatus::PendingOperations => ("awaiting_ops", "待运营审批", "warning"),
        ReviewStatus::Approved => ("effective", "已生效", "success"),
        ReviewStatus::NotSubmitted => ("draft", "草稿", "neutral"),
    }
}

/// 结案资格判定（服务端权威；移植自 erp-client `close-eligibility.ts::computeCloseEligibility`）。
///
/// 规则（W05 §5.3/§12）：非卡券以客户验收完成判定交付；卡券以履约期限到期判定
/// （不因已消费完提前算完成，`FulfillmentProgress::Completed` 由到期任务写入）；
/// 结案门槛为交付完成且回款收齐，开票进度不参与。回款是否收齐优先看
/// `collection_progress`，辅以应收子账合计兜底（两者任一满足即视为收齐，
/// 与前端原实现的双重判断保持一致）。
#[allow(clippy::too_many_arguments)]
pub(super) fn compute_close_eligibility(
    business_type: BusinessType,
    commercial: CommercialStatus,
    close: CloseStatus,
    fulfillment: FulfillmentProgress,
    collection: CollectionProgress,
    invoice: InvoiceProgress,
    settled_total: Amount,
    gross_total: Amount,
) -> dto::CloseEligibilityView {
    let invoice_complete = invoice == InvoiceProgress::Completed;
    let fulfillment_done = fulfillment == FulfillmentProgress::Completed;
    let collection_settled = collection == CollectionProgress::Settled;

    if close == CloseStatus::Closed
        || commercial == CommercialStatus::Voided
        || commercial == CommercialStatus::Draft
    {
        let closed = close == CloseStatus::Closed;
        let (blockers, note) = if closed {
            (
                Vec::new(),
                "交付与回款都已完成，本单已自动结案。开票是否做完不影响结案。".to_string(),
            )
        } else if commercial == CommercialStatus::Voided {
            (
                vec!["本单已作废，不会再结案".to_string()],
                "作废单只保留历史记录，不能结案，也不能恢复。".to_string(),
            )
        } else {
            (
                vec!["草稿尚未生效，谈不上结案".to_string()],
                "草稿还没生效，先完成提交与确认。".to_string(),
            )
        };
        return dto::CloseEligibilityView {
            fulfillment_complete: closed || fulfillment_done,
            receivable_settled: closed || collection_settled,
            invoice_complete,
            eligible_to_close: closed,
            blockers,
            note,
        };
    }

    let fulfillment_complete = fulfillment_done;
    let receivable_settled = collection_settled || settled_total >= gross_total;
    let mut blockers = Vec::new();
    if !fulfillment_complete {
        blockers.push(
            if business_type == BusinessType::Voucher {
                "卡券还没到履约期限（持卡人是否消费完都不提前算交付完成）"
            } else {
                "客户验收还没做完"
            }
            .to_string(),
        );
    }
    if !receivable_settled {
        blockers.push("客户回款还没收齐".to_string());
    }
    let eligible_to_close = fulfillment_complete && receivable_settled;
    let note = if eligible_to_close {
        "交付和回款都齐了，系统会自动结案。发票开没开完都不挡结案，也无需人工点「关闭」。".to_string()
    } else {
        format!("还不能结案：{}。开票进度不参与是否结案。", blockers.join("；"))
    };

    dto::CloseEligibilityView {
        fulfillment_complete,
        receivable_settled,
        invoice_complete,
        eligible_to_close,
        blockers,
        note,
    }
}

/// 是否可以发起销售变更单（服务端权威；移植自 close-eligibility.ts::canStartSalesChange）。
///
/// 已生效单不可直接编辑；ERP 开单的已生效单可发起销售变更（商城开单同步的
/// 单据、尚在确认/审批中的单据、已有进行中变更单的单据均不可发起）。
pub(super) fn compute_can_start_sales_change(
    origin_system: OriginSystem,
    stage_code: &str,
    stage_label: &str,
    has_active_change_order: bool,
) -> (bool, Option<String>) {
    if origin_system != OriginSystem::Erp {
        return (
            false,
            Some("这单由商城开单，商业数据同步中，本系统只能查看；改内容请在商城处理。".to_string()),
        );
    }
    if matches!(stage_code, "draft" | "voided" | "closed") {
        return (
            false,
            Some(format!("当前状态是「{stage_label}」，不能发起改单。")),
        );
    }
    if matches!(
        stage_code,
        "awaiting_sales" | "awaiting_confirm" | "awaiting_sales_lead" | "awaiting_ops"
    ) {
        return (
            false,
            Some("本单还在确认/审批中，请先处理完当前待办，再发起改单。".to_string()),
        );
    }
    if has_active_change_order {
        return (
            false,
            Some("已有一笔改单在处理中，请等它走完再发起新的。".to_string()),
        );
    }
    (true, None)
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use entities::money::Amount;
    use entities::sales_order::{
        BusinessType, CloseStatus, CollectionProgress, CommercialStatus, FulfillmentProgress,
        InvoiceProgress, OriginSystem, ReviewStatus,
    };

    use super::{
        compute_can_start_sales_change, compute_close_eligibility, detail_owner_user_id,
        stage_code_label_tone,
    };

    #[test]
    fn detail_owner_prefers_working_copy_then_latest_submission_then_creator() {
        assert_eq!(
            detail_owner_user_id(Some("editor"), Some("submitter"), "creator"),
            "editor"
        );
        assert_eq!(
            detail_owner_user_id(None, Some("submitter"), "creator"),
            "submitter"
        );
        assert_eq!(detail_owner_user_id(None, None, "creator"), "creator");
    }

    #[test]
    fn stage_closed_overrides_everything() {
        let (code, label, tone) = stage_code_label_tone(
            CommercialStatus::Effective,
            ReviewStatus::Approved,
            CloseStatus::Closed,
            FulfillmentProgress::Completed,
        );
        assert_eq!((code, label, tone), ("closed", "已关闭", "void"));
    }

    #[test]
    fn stage_voided() {
        let (code, label, tone) = stage_code_label_tone(
            CommercialStatus::Voided,
            ReviewStatus::NotSubmitted,
            CloseStatus::NotSatisfied,
            FulfillmentProgress::NotStarted,
        );
        assert_eq!((code, label, tone), ("voided", "已作废", "void"));
    }

    #[test]
    fn stage_effective_distinguishes_fulfilling() {
        let fulfilling = stage_code_label_tone(
            CommercialStatus::Effective,
            ReviewStatus::Approved,
            CloseStatus::NotSatisfied,
            FulfillmentProgress::PartiallyFulfilled,
        );
        assert_eq!(fulfilling, ("fulfilling", "履约中", "info"));

        let effective = stage_code_label_tone(
            CommercialStatus::Effective,
            ReviewStatus::Approved,
            CloseStatus::NotSatisfied,
            FulfillmentProgress::NotStarted,
        );
        assert_eq!(effective, ("effective", "已生效", "success"));
    }

    #[test]
    fn stage_pending_review_branches() {
        let cases = [
            (
                ReviewStatus::PendingProcurementConfirmation,
                ("awaiting_confirm", "待二次确认", "warning"),
            ),
            (
                ReviewStatus::PendingLowMarginSuperior,
                ("awaiting_sales", "待销售处理", "warning"),
            ),
            (
                ReviewStatus::Rejected,
                ("awaiting_sales", "待销售处理", "warning"),
            ),
            (
                ReviewStatus::PendingSalesLeader,
                ("awaiting_sales_lead", "待销售领导审批", "warning"),
            ),
            (
                ReviewStatus::PendingOperations,
                ("awaiting_ops", "待运营审批", "warning"),
            ),
            (ReviewStatus::Approved, ("effective", "已生效", "success")),
            (ReviewStatus::NotSubmitted, ("draft", "草稿", "neutral")),
        ];
        for (review, expected) in cases {
            let actual = stage_code_label_tone(
                CommercialStatus::PendingReview,
                review,
                CloseStatus::NotSatisfied,
                FulfillmentProgress::NotStarted,
            );
            assert_eq!(actual, expected, "review={review:?}");
        }
    }

    #[test]
    fn close_eligibility_closed_branch_is_always_eligible() {
        let view = compute_close_eligibility(
            BusinessType::GoodsService,
            CommercialStatus::Effective,
            CloseStatus::Closed,
            FulfillmentProgress::NotStarted,
            CollectionProgress::NotCollected,
            InvoiceProgress::NotInvoiced,
            Amount::from_str("0").unwrap(),
            Amount::from_str("100").unwrap(),
        );
        assert!(view.eligible_to_close);
        assert!(view.fulfillment_complete);
        assert!(view.receivable_settled);
        assert!(view.blockers.is_empty());
    }

    #[test]
    fn close_eligibility_voided_and_draft_block_with_reason() {
        let voided = compute_close_eligibility(
            BusinessType::GoodsService,
            CommercialStatus::Voided,
            CloseStatus::NotSatisfied,
            FulfillmentProgress::NotStarted,
            CollectionProgress::NotCollected,
            InvoiceProgress::NotInvoiced,
            Amount::from_str("0").unwrap(),
            Amount::from_str("100").unwrap(),
        );
        assert!(!voided.eligible_to_close);
        assert_eq!(voided.blockers, vec!["本单已作废，不会再结案".to_string()]);

        let draft = compute_close_eligibility(
            BusinessType::GoodsService,
            CommercialStatus::Draft,
            CloseStatus::NotSatisfied,
            FulfillmentProgress::NotStarted,
            CollectionProgress::NotCollected,
            InvoiceProgress::NotInvoiced,
            Amount::from_str("0").unwrap(),
            Amount::from_str("100").unwrap(),
        );
        assert!(!draft.eligible_to_close);
        assert_eq!(draft.blockers, vec!["草稿尚未生效，谈不上结案".to_string()]);
    }

    #[test]
    fn close_eligibility_goods_vs_voucher_blocker_text() {
        let goods = compute_close_eligibility(
            BusinessType::GoodsService,
            CommercialStatus::Effective,
            CloseStatus::NotSatisfied,
            FulfillmentProgress::NotStarted,
            CollectionProgress::NotCollected,
            InvoiceProgress::NotInvoiced,
            Amount::from_str("0").unwrap(),
            Amount::from_str("100").unwrap(),
        );
        assert_eq!(goods.blockers[0], "客户验收还没做完");

        let voucher = compute_close_eligibility(
            BusinessType::Voucher,
            CommercialStatus::Effective,
            CloseStatus::NotSatisfied,
            FulfillmentProgress::NotStarted,
            CollectionProgress::NotCollected,
            InvoiceProgress::NotInvoiced,
            Amount::from_str("0").unwrap(),
            Amount::from_str("100").unwrap(),
        );
        assert_eq!(
            voucher.blockers[0],
            "卡券还没到履约期限（持卡人是否消费完都不提前算交付完成）"
        );
    }

    #[test]
    fn close_eligibility_receivable_settled_falls_back_to_amount_comparison() {
        // collection_progress 还没被标记 SETTLED，但应收子账合计已收齐——
        // 与前端原实现的双重判断保持一致。
        let view = compute_close_eligibility(
            BusinessType::GoodsService,
            CommercialStatus::Effective,
            CloseStatus::NotSatisfied,
            FulfillmentProgress::Completed,
            CollectionProgress::PartiallyCollected,
            InvoiceProgress::NotInvoiced,
            Amount::from_str("100").unwrap(),
            Amount::from_str("100").unwrap(),
        );
        assert!(view.receivable_settled);
        assert!(view.eligible_to_close);
    }

    #[test]
    fn close_eligibility_invoice_progress_never_blocks() {
        let view = compute_close_eligibility(
            BusinessType::GoodsService,
            CommercialStatus::Effective,
            CloseStatus::NotSatisfied,
            FulfillmentProgress::Completed,
            CollectionProgress::Settled,
            InvoiceProgress::NotInvoiced,
            Amount::from_str("100").unwrap(),
            Amount::from_str("100").unwrap(),
        );
        assert!(view.eligible_to_close);
        assert!(!view.invoice_complete);
    }

    #[test]
    fn can_start_sales_change_blocks_mall_origin() {
        let (allowed, reason) =
            compute_can_start_sales_change(OriginSystem::Mall, "effective", "已生效", false);
        assert!(!allowed);
        assert!(reason.unwrap().contains("商城"));
    }

    #[test]
    fn can_start_sales_change_blocks_terminal_and_pending_stages() {
        for code in ["draft", "voided", "closed"] {
            let (allowed, reason) = compute_can_start_sales_change(OriginSystem::Erp, code, "任意", false);
            assert!(!allowed, "stage={code}");
            assert!(reason.is_some());
        }
        for code in [
            "awaiting_sales",
            "awaiting_confirm",
            "awaiting_sales_lead",
            "awaiting_ops",
        ] {
            let (allowed, _) = compute_can_start_sales_change(OriginSystem::Erp, code, "任意", false);
            assert!(!allowed, "stage={code}");
        }
    }

    #[test]
    fn can_start_sales_change_blocks_active_change_order_else_allows() {
        let (allowed, reason) =
            compute_can_start_sales_change(OriginSystem::Erp, "effective", "已生效", true);
        assert!(!allowed);
        assert!(reason.unwrap().contains("处理中"));

        let (allowed, reason) =
            compute_can_start_sales_change(OriginSystem::Erp, "effective", "已生效", false);
        assert!(allowed);
        assert!(reason.is_none());
    }
}
