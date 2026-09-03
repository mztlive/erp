//! 销售单阶段判定：阶段 `(code, label, tone)`、结案资格、销售变更发起资格。

use entities::sales_order::{
    CloseStatus, CommercialStatus, FulfillmentProgress, OriginSystem, ReviewStatus,
    SalesOrderClosureAssessment, SalesOrderClosureTerminal, SalesOrderFulfillmentBlocker,
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
        ReviewStatus::InApproval => ("in_approval", "审批中", "warning"),
    }
}

/// 把实体结案事实映射为中文结案资格视图。
///
/// 规则（W05 §5.3/§12）由 `SalesOrderClosureFacts` 承载；本函数只负责阻断原因
/// 与说明文案。开票进度永不阻断。
///
/// # 参数
/// * `assessment` - 实体结案资格纯事实
///
/// # 返回
/// 返回结案资格视图。
///
/// # 错误
/// 无。
///
/// # 关键业务约束
/// 不得在本函数内比较应收金额或解释终态；无子账零额比较已由实体拒绝。
pub(super) fn close_eligibility_view(assessment: SalesOrderClosureAssessment) -> dto::CloseEligibilityView {
    let (blockers, note) = match assessment.terminal {
        Some(SalesOrderClosureTerminal::Closed) => (
            Vec::new(),
            "交付与回款都已完成，本单已自动结案。开票是否做完不影响结案。".to_string(),
        ),
        Some(SalesOrderClosureTerminal::Voided) => (
            vec!["本单已作废，不会再结案".to_string()],
            "作废单只保留历史记录，不能结案，也不能恢复。".to_string(),
        ),
        Some(SalesOrderClosureTerminal::Draft) => (
            vec!["草稿尚未生效，谈不上结案".to_string()],
            "草稿还没生效，先完成提交与确认。".to_string(),
        ),
        None => {
            let mut blockers = Vec::new();
            if let Some(blocker) = assessment.fulfillment_blocker {
                blockers.push(
                    match blocker {
                        SalesOrderFulfillmentBlocker::VoucherExpiry => {
                            "卡券还没到履约期限（持卡人是否消费完都不提前算交付完成）"
                        }
                        SalesOrderFulfillmentBlocker::CustomerAcceptance => "客户验收还没做完",
                    }
                    .to_string(),
                );
            }
            if assessment.collection_unsettled {
                blockers.push("客户回款还没收齐".to_string());
            }
            let note = if assessment.eligible_to_close {
                "交付和回款都齐了，系统会自动结案。发票开没开完都不挡结案，也无需人工点「关闭」。".to_string()
            } else {
                format!("还不能结案：{}。开票进度不参与是否结案。", blockers.join("；"))
            };
            (blockers, note)
        }
    };
    dto::CloseEligibilityView {
        fulfillment_complete: assessment.fulfillment_complete,
        receivable_settled: assessment.receivable_settled,
        invoice_complete: assessment.invoice_complete,
        eligible_to_close: assessment.eligible_to_close,
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
        "awaiting_sales" | "awaiting_confirm" | "awaiting_sales_lead" | "awaiting_ops" | "in_approval"
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

    use entities::ids::{CustomerAccountId, PartyId, SalesOrderId};
    use entities::money::Amount;
    use entities::sales_order::{
        BusinessType, CloseStatus, CollectionProgress, CommercialStatus, FulfillmentProgress,
        InvoiceProgress, OriginSystem, ReviewStatus, SalesOrder, SalesOrderData,
    };

    use super::{
        close_eligibility_view, compute_can_start_sales_change, detail_owner_user_id, stage_code_label_tone,
    };

    fn amt(value: &str) -> Amount {
        Amount::from_str(value).unwrap()
    }

    fn order(business_type: BusinessType) -> SalesOrder {
        SalesOrder::new(
            SalesOrderId::new("o-1"),
            SalesOrderData {
                order_no: "SO-1".to_string(),
                business_type,
                origin_system: OriginSystem::Erp,
                source_identity_id: None,
                customer_id: CustomerAccountId::new("cust-1"),
                contract_id: None,
                settlement_party_id: PartyId::new("party-1"),
                source_status_code: None,
            },
            "admin-1",
        )
        .unwrap()
    }

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
            (ReviewStatus::InApproval, ("in_approval", "审批中", "warning")),
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
    fn close_eligibility_view_maps_terminal_and_blocker_copy() {
        let mut closed = order(BusinessType::GoodsService);
        closed.commercial_status = CommercialStatus::Effective;
        closed.close_status = CloseStatus::Closed;
        let closed_view = close_eligibility_view(closed.closure_facts().assess(true, amt("0"), amt("100")));
        assert!(closed_view.eligible_to_close);
        assert!(closed_view.blockers.is_empty());

        let mut voided = order(BusinessType::GoodsService);
        voided.commercial_status = CommercialStatus::Voided;
        let voided_view = close_eligibility_view(voided.closure_facts().assess(false, amt("0"), amt("0")));
        assert_eq!(voided_view.blockers, vec!["本单已作废，不会再结案".to_string()]);

        let draft_view = close_eligibility_view(order(BusinessType::GoodsService).closure_facts().assess(
            false,
            amt("0"),
            amt("0"),
        ));
        assert_eq!(draft_view.blockers, vec!["草稿尚未生效，谈不上结案".to_string()]);

        let mut goods = order(BusinessType::GoodsService);
        goods.commercial_status = CommercialStatus::Effective;
        let goods_view = close_eligibility_view(goods.closure_facts().assess(true, amt("0"), amt("100")));
        assert_eq!(goods_view.blockers[0], "客户验收还没做完");

        let mut voucher = order(BusinessType::Voucher);
        voucher.commercial_status = CommercialStatus::Effective;
        let voucher_view = close_eligibility_view(voucher.closure_facts().assess(true, amt("0"), amt("100")));
        assert_eq!(
            voucher_view.blockers[0],
            "卡券还没到履约期限（持卡人是否消费完都不提前算交付完成）"
        );
    }

    #[test]
    fn close_eligibility_view_keeps_amount_fallback_and_invoice_copy() {
        let mut order = order(BusinessType::GoodsService);
        order.commercial_status = CommercialStatus::Effective;
        order.fulfillment_progress = FulfillmentProgress::Completed;
        order.collection_progress = CollectionProgress::PartiallyCollected;
        let settled = close_eligibility_view(order.closure_facts().assess(true, amt("100"), amt("100")));
        assert!(settled.receivable_settled);
        assert!(settled.eligible_to_close);

        let missing = close_eligibility_view(order.closure_facts().assess(false, amt("0"), amt("0")));
        assert!(!missing.receivable_settled);
        assert!(!missing.eligible_to_close);
        assert!(missing.blockers.iter().any(|item| item.contains("回款")));

        let short = close_eligibility_view(order.closure_facts().assess(true, amt("50"), amt("100")));
        assert!(short.fulfillment_complete);
        assert!(!short.receivable_settled);
        assert!(!short.eligible_to_close);
        assert_eq!(short.blockers, vec!["客户回款还没收齐".to_string()]);

        order.collection_progress = CollectionProgress::Settled;
        order.invoice_progress = InvoiceProgress::NotInvoiced;
        let invoicing = close_eligibility_view(order.closure_facts().assess(true, amt("100"), amt("100")));
        assert!(invoicing.eligible_to_close);
        assert!(!invoicing.invoice_complete);
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
            "in_approval",
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
