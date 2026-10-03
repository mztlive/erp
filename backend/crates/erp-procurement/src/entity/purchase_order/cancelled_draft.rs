//! 审批取消后从冻结采购提交派生独立可编辑草稿，保留原提交及明细。

use std::collections::HashSet;

use erp_core::ids::{PurchaseOrderSubmissionId, PurchaseOrderSubmissionLineId};
use erp_core::{Error, Result};

use super::{
    PurchaseOrder, PurchaseOrderStatus, PurchaseOrderSubmission, PurchaseOrderSubmissionData,
    PurchaseOrderSubmissionLine, SubmissionStatus,
};

/// 取消审批事务需要新建的独立采购草稿。
pub(crate) struct CancelledPurchaseDraft {
    /// 可编辑草稿头，使用新的提交身份。
    pub(crate) submission: PurchaseOrderSubmission,
    /// 可编辑草稿行，保留原销售稳定行和来源快照。
    pub(crate) lines: Vec<PurchaseOrderSubmissionLine>,
}

impl PurchaseOrder {
    /// 从真正取消审批的采购单派生独立草稿并切换当前提交指针。
    ///
    /// # 参数
    /// * `previous` - 同一事务重读的取消前采购单
    /// * `source` - 取消前采购单引用的冻结提交
    /// * `source_lines` - 原冻结提交的完整明细
    /// * `draft_id` - 新草稿提交身份
    /// * `line_ids` - 与原明细一一对应的新草稿行身份
    ///
    /// # 返回
    /// 返回独立草稿头和完整行；仅成功时切换本采购单的当前提交指针。
    ///
    /// # 错误
    /// 取消状态、版本、来源或新身份不一致，明细金额不守恒时返回领域错误。
    pub(crate) fn reopen_cancelled_draft(
        &mut self,
        previous: &Self,
        source: &PurchaseOrderSubmission,
        source_lines: &[PurchaseOrderSubmissionLine],
        draft_id: PurchaseOrderSubmissionId,
        line_ids: &[PurchaseOrderSubmissionLineId],
    ) -> Result<CancelledPurchaseDraft> {
        self.ensure_cancelled_draft_source(previous, source)?;
        source.ensure_line_totals(source_lines)?;
        ensure_new_draft_identities(source, source_lines, &draft_id, line_ids)?;
        let submission = reopened_header(source, draft_id.clone())?;
        let lines = source_lines
            .iter()
            .zip(line_ids)
            .map(|(line, id)| {
                PurchaseOrderSubmissionLine::freeze_from_draft(id.clone(), draft_id.clone(), line)
            })
            .collect::<Result<Vec<_>>>()?;
        self.current_submission_id = Some(draft_id.to_string());
        Ok(CancelledPurchaseDraft { submission, lines })
    }

    /// 校验传入采购单恰为同一版本执行取消动作后的结果，且来源仍为冻结提交。
    fn ensure_cancelled_draft_source(&self, previous: &Self, source: &PurchaseOrderSubmission) -> Result<()> {
        if previous.stable.status != PurchaseOrderStatus::InApproval
            || previous.approval_subject_version == 0
            || previous.stable.current_revision_id.is_some()
            || previous.base.is_deleted()
            || source.base.is_deleted()
        {
            return Err(Error::from("采购单不是可恢复草稿的审批提交"));
        }
        let mut cancelled = previous.clone();
        cancelled.cancel_approval(self.stable.updated_by.clone())?;
        if &cancelled != self {
            return Err(Error::from("采购单取消版本或内容已变化"));
        }
        if source.purchase_order_id.as_ref() != self.base.id
            || self.current_submission_id.as_deref() != Some(source.base.id.as_str())
            || source.supplier_id != self.supplier_id
            || source.purchase_type != self.purchase_type
            || source.fulfillment_responsibility != self.fulfillment_responsibility
            || source.payment_term_snapshot.payment_term_code != self.payment_term_code
            || !matches!(source.status, SubmissionStatus::Pending | SubmissionStatus::Rejected)
            || source.submitted_at.is_none()
            || source.submitted_by.is_none()
        {
            return Err(Error::from("采购取消来源不是当前冻结提交"));
        }
        Ok(())
    }
}

/// 校验新草稿头行使用独立身份，禁止覆盖或复用历史冻结行。
fn ensure_new_draft_identities(
    source: &PurchaseOrderSubmission,
    source_lines: &[PurchaseOrderSubmissionLine],
    draft_id: &PurchaseOrderSubmissionId,
    line_ids: &[PurchaseOrderSubmissionLineId],
) -> Result<()> {
    let source_ids = source_lines.iter().map(|line| line.base.id.as_str()).collect::<HashSet<_>>();
    let new_ids = line_ids.iter().map(AsRef::as_ref).collect::<HashSet<_>>();
    if draft_id.as_ref() == source.base.id
        || source_lines.is_empty()
        || source_ids.len() != source_lines.len()
        || source_lines.iter().any(|line| line.base.is_deleted())
        || line_ids.len() != source_lines.len()
        || new_ids.len() != line_ids.len()
        || new_ids.iter().any(|id| source_ids.contains(id))
    {
        return Err(Error::from("采购取消草稿必须使用独立的完整头行身份"));
    }
    Ok(())
}

/// 复制冻结业务资料并通过构造函数清除提交、审核审计，生成可编辑草稿头。
fn reopened_header(
    source: &PurchaseOrderSubmission,
    draft_id: PurchaseOrderSubmissionId,
) -> Result<PurchaseOrderSubmission> {
    let submission_no = format!("DRAFT-{draft_id}");
    PurchaseOrderSubmission::new(
        draft_id,
        PurchaseOrderSubmissionData {
            purchase_order_id: source.purchase_order_id.clone(),
            submission_no,
            supplier_id: source.supplier_id.clone(),
            purchase_type: source.purchase_type,
            fulfillment_responsibility: source.fulfillment_responsibility,
            supplier_revision_id: source.supplier_revision_id.clone(),
            supplier_snapshot: source.supplier_snapshot.clone(),
            payment_term_snapshot: source.payment_term_snapshot.clone(),
            gross_amount: source.gross_amount,
            net_amount: source.net_amount,
            tax_amount: source.tax_amount,
        },
    )
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use entity_core::BaseModel;
    use erp_core::common::time::{BusinessDate, Instant};
    use erp_core::ids::{
        ProcurementConfirmationLineId, PurchaseOrderId, SalesOrderId, SalesOrderLineId, SalesOrderRevisionId,
        SalesOrderRevisionLineId, SalesOrderSubmissionLineId, SkuId, SkuRevisionId, SupplierAccountId,
        SupplierCommercialProfileRevisionId,
    };
    use erp_core::money::{Amount, Quantity, Rate, UnitPrice, line_amounts};

    use super::*;
    use crate::dto::purchase_order::SavePurchaseOrderLinePatch;
    use crate::entity::purchase_order::{
        FulfillmentResponsibility, PaymentTermSnapshot, PurchaseLineType, PurchaseOrderData,
        PurchaseOrderReviewDecision, PurchaseOrderSubmissionLineData, PurchaseType, SupplierSnapshot,
    };
    use crate::service::purchase_order::draft_edit::build_draft_replacement;

    /// 构造金额与来源一致的商品行和物流费用行。
    fn source_lines() -> Vec<PurchaseOrderSubmissionLine> {
        let (gross, net, tax) = line_amounts(
            UnitPrice::from_str("100").unwrap(),
            Quantity::from_str("2").unwrap(),
            Rate::from_str("0.13").unwrap(),
        );
        let item = PurchaseOrderSubmissionLine::new(
            PurchaseOrderSubmissionLineId::new("old-item"),
            PurchaseOrderSubmissionLineData {
                purchase_order_submission_id: PurchaseOrderSubmissionId::new("formal-1"),
                line_no: 1,
                line_type: PurchaseLineType::ItemService,
                procurement_confirmation_line_id: Some(ProcurementConfirmationLineId::new("confirmation-1")),
                sku_id: Some(SkuId::new("sku-1")),
                sku_revision_id: Some(SkuRevisionId::new("sku-revision-1")),
                product_name_snapshot: Some("采购商品".into()),
                specification_snapshot: Some("500g".into()),
                quantity: Some(Quantity::from_str("2").unwrap()),
                base_unit_code: Some("EA".into()),
                unit_cost_gross: Some(UnitPrice::from_str("100").unwrap()),
                gross_amount: gross,
                net_amount: net,
                tax_amount: tax,
                input_tax_rate: Some(Rate::from_str("0.13").unwrap()),
                expected_delivery_date: Some(BusinessDate::from_ymd(2026, 10, 5).unwrap()),
                sales_order_line_id: Some(SalesOrderLineId::new("sales-line-1")),
                sales_order_revision_line_id: Some(SalesOrderRevisionLineId::new("sales-revision-line-1")),
                sales_order_submission_line_id: Some(SalesOrderSubmissionLineId::new(
                    "sales-submission-line-1",
                )),
                allocated_quantity: Some(Quantity::from_str("2").unwrap()),
            },
        )
        .unwrap();
        let mut logistics = item.clone();
        logistics.base = BaseModel { id: "old-logistics".into(), ..BaseModel::fake() };
        logistics.line_no = 2;
        logistics.line_type = PurchaseLineType::LogisticsFee;
        logistics.procurement_confirmation_line_id = None;
        logistics.sku_id = None;
        logistics.sku_revision_id = None;
        logistics.product_name_snapshot = None;
        logistics.specification_snapshot = None;
        logistics.quantity = None;
        logistics.base_unit_code = None;
        logistics.unit_cost_gross = None;
        logistics.gross_amount = Amount::from_str("10").unwrap();
        logistics.net_amount = logistics.gross_amount;
        logistics.tax_amount = Amount::from_str("0").unwrap();
        logistics.input_tax_rate = Some(Rate::from_str("0").unwrap());
        logistics.sales_order_line_id = None;
        logistics.sales_order_revision_line_id = None;
        logistics.sales_order_submission_line_id = None;
        logistics.allocated_quantity = None;
        vec![item, logistics]
    }

    /// 构造真实提交及取消动作生成的同版本采购聚合。
    fn fixture() -> (PurchaseOrder, PurchaseOrder, PurchaseOrderSubmission, Vec<PurchaseOrderSubmissionLine>)
    {
        let lines = source_lines();
        let mut order = PurchaseOrder::new(
            PurchaseOrderId::new("po-1"),
            PurchaseOrderData {
                business_org_unit_id: "procurement-org".into(),
                purchase_no: "PO-1".into(),
                sales_order_id: SalesOrderId::new("so-1"),
                sales_order_revision_id: SalesOrderRevisionId::new("sales-revision-1"),
                creation_basis_id: "basis-1".into(),
                supplier_id: SupplierAccountId::new("supplier-1"),
                purchase_type: PurchaseType::Physical,
                payment_term_code: "POSTPAY_NET30".into(),
                fulfillment_responsibility: FulfillmentResponsibility::SupplierDirect,
                owner_user_id: "buyer-1".into(),
                target_warehouse_id: None,
            },
            "buyer-1",
            crate::entity::test_support::payment_term,
        )
        .unwrap();
        order.base = BaseModel { id: "po-1".into(), version: 3, ..BaseModel::fake() };
        let mut source = PurchaseOrderSubmission::new(
            PurchaseOrderSubmissionId::new("formal-1"),
            PurchaseOrderSubmissionData {
                purchase_order_id: PurchaseOrderId::new("po-1"),
                submission_no: "SUB-000001".into(),
                supplier_id: SupplierAccountId::new("supplier-1"),
                purchase_type: order.purchase_type,
                fulfillment_responsibility: order.fulfillment_responsibility,
                supplier_revision_id: SupplierCommercialProfileRevisionId::new("supplier-revision-1"),
                supplier_snapshot: SupplierSnapshot::new("采购供应商".into()).unwrap(),
                payment_term_snapshot: PaymentTermSnapshot::new(
                    "POSTPAY_NET30".into(),
                    false,
                    None,
                    None,
                    crate::entity::test_support::payment_term,
                )
                .unwrap(),
                gross_amount: lines[0].gross_amount.checked_add(lines[1].gross_amount),
                net_amount: lines[0].net_amount.checked_add(lines[1].net_amount),
                tax_amount: lines[0].tax_amount.checked_add(lines[1].tax_amount),
            },
        )
        .unwrap();
        source.base = BaseModel { id: "formal-1".into(), ..BaseModel::fake() };
        source.submit(Instant::from_unix_secs(1_700_000_000), "buyer-1").unwrap();
        order.start_approval(source.base.id.clone(), "buyer-1").unwrap();
        let mut cancelled = order.clone();
        cancelled.cancel_approval("buyer-1").unwrap();
        (order, cancelled, source, lines)
    }

    /// 执行实际生产恢复规则，使用固定且独立的草稿头行身份。
    fn reopen(
        cancelled: &mut PurchaseOrder,
        previous: &PurchaseOrder,
        source: &PurchaseOrderSubmission,
        lines: &[PurchaseOrderSubmissionLine],
    ) -> Result<CancelledPurchaseDraft> {
        cancelled.reopen_cancelled_draft(
            previous,
            source,
            lines,
            PurchaseOrderSubmissionId::new("draft-2"),
            &[
                PurchaseOrderSubmissionLineId::new("draft-item"),
                PurchaseOrderSubmissionLineId::new("draft-logistics"),
            ],
        )
    }

    #[test]
    fn cancelled_submission_reopens_independent_editable_head_and_all_lines() {
        let (previous, mut cancelled, source, lines) = fixture();
        let source_before = source.clone();
        let lines_before = lines.clone();
        let draft = reopen(&mut cancelled, &previous, &source, &lines).unwrap();
        assert_eq!(cancelled.base, previous.base);
        assert_eq!(cancelled.purchase_no, previous.purchase_no);
        assert_eq!(cancelled.approval_subject_version, 1);
        assert_eq!(cancelled.draft_submission_id().unwrap().as_ref(), "draft-2");
        assert_eq!(draft.submission.status, SubmissionStatus::Draft);
        assert_eq!(draft.submission.submission_no, "DRAFT-draft-2");
        assert_eq!(draft.submission.supplier_revision_id, source.supplier_revision_id);
        assert_eq!(draft.submission.supplier_snapshot, source.supplier_snapshot);
        assert_eq!(draft.submission.payment_term_snapshot, source.payment_term_snapshot);
        assert!(draft.submission.submitted_at.is_none());
        assert!(draft.submission.submitted_by.is_none());
        assert!(draft.submission.reviewed_at.is_none());
        assert!(draft.submission.reviewed_by.is_none());
        assert!(draft.submission.review_reason_code.is_none());
        assert!(draft.submission.review_comment.is_none());
        draft.submission.ensure_line_totals(&draft.lines).unwrap();
        for (old, new) in lines.iter().zip(&draft.lines) {
            let mut expected = old.clone();
            expected.base = new.base.clone();
            expected.purchase_order_submission_id = PurchaseOrderSubmissionId::new("draft-2");
            assert_eq!(new, &expected);
            assert_ne!(new.base.id, old.base.id);
        }
        assert_eq!(source, source_before);
        assert_eq!(lines, lines_before);
        assert!(source.ensure_draft().is_err());
    }

    #[test]
    fn cancelled_rejected_snapshot_reopens_without_carrying_old_review_to_new_submission() {
        let (previous, mut cancelled, mut source, lines) = fixture();
        source
            .record_review(
                PurchaseOrderReviewDecision::Rejected {
                    reason_code: "COST".into(),
                    comment: Some("核实单价".into()),
                },
                Instant::from_unix_secs(1_700_000_010),
                "finance-1",
            )
            .unwrap();
        let historical = source.clone();
        let draft = reopen(&mut cancelled, &previous, &source, &lines).unwrap();
        let formal = PurchaseOrderSubmission::freeze_from_draft(
            PurchaseOrderSubmissionId::new("formal-2"),
            PurchaseOrderSubmission::next_submission_no(&[source.clone(), draft.submission.clone()]).unwrap(),
            &draft.submission,
            Instant::from_unix_secs(1_700_000_100),
            "buyer-1",
        )
        .unwrap();
        assert_eq!(formal.submission_no, "SUB-000002");
        assert_eq!(formal.status, SubmissionStatus::Pending);
        assert!(formal.review_reason_code.is_none());
        assert_eq!(cancelled.start_approval(formal.base.id.clone(), "buyer-1").unwrap(), 2);
        let next_previous = cancelled.clone();
        cancelled.cancel_approval("buyer-1").unwrap();
        let formal_lines = draft
            .lines
            .iter()
            .enumerate()
            .map(|(index, line)| {
                PurchaseOrderSubmissionLine::freeze_from_draft(
                    format!("formal-2-line-{index}").into(),
                    PurchaseOrderSubmissionId::new("formal-2"),
                    line,
                )
                .unwrap()
            })
            .collect::<Vec<_>>();
        let third_draft = cancelled
            .reopen_cancelled_draft(
                &next_previous,
                &formal,
                &formal_lines,
                PurchaseOrderSubmissionId::new("draft-3"),
                &[
                    PurchaseOrderSubmissionLineId::new("draft-3-item"),
                    PurchaseOrderSubmissionLineId::new("draft-3-logistics"),
                ],
            )
            .unwrap();
        assert_eq!(cancelled.approval_subject_version, 2);
        assert_eq!(third_draft.submission.status, SubmissionStatus::Draft);
        assert_eq!(cancelled.current_submission_id.as_deref(), Some("draft-3"));
        assert_eq!(source, historical);
        assert!(formal.ensure_draft().is_err());
    }

    #[test]
    fn reopened_draft_accepts_current_line_cost_edits_then_freezes_a_new_snapshot() {
        let (previous, mut cancelled, source, lines) = fixture();
        let draft = reopen(&mut cancelled, &previous, &source, &lines).unwrap();
        let patches = vec![
            SavePurchaseOrderLinePatch {
                line_id: "draft-item".into(),
                line_type: PurchaseLineType::ItemService,
                quantity: Some("2".into()),
                unit_cost_gross: Some("99".into()),
                input_tax_rate: None,
            },
            SavePurchaseOrderLinePatch {
                line_id: "draft-logistics".into(),
                line_type: PurchaseLineType::LogisticsFee,
                quantity: None,
                unit_cost_gross: None,
                input_tax_rate: None,
            },
        ];
        let requested = SavePurchaseOrderLinePatch::resolve_all(&patches, &draft.lines).unwrap();
        let replacement = build_draft_replacement(&cancelled, &draft.submission, &requested).unwrap();
        replacement.submission.ensure_draft().unwrap();
        replacement.submission.ensure_line_totals(&replacement.lines).unwrap();
        assert_eq!(replacement.submission.gross_amount, Amount::from_str("208").unwrap());
        assert_eq!(replacement.lines[0].unit_cost_gross, Some(UnitPrice::from_str("99").unwrap()));
        assert_eq!(replacement.lines[0].quantity, lines[0].quantity);
        assert_eq!(replacement.lines[0].sales_order_line_id, lines[0].sales_order_line_id);
        let formal = PurchaseOrderSubmission::freeze_from_draft(
            PurchaseOrderSubmissionId::new("formal-2"),
            "SUB-000002".into(),
            &replacement.submission,
            Instant::from_unix_secs(1_700_000_100),
            "buyer-1",
        )
        .unwrap();
        assert_eq!(formal.status, SubmissionStatus::Pending);
        assert_eq!(formal.gross_amount, Amount::from_str("208").unwrap());
        assert_eq!(source.gross_amount, Amount::from_str("210").unwrap());
        assert_eq!(lines[0].unit_cost_gross, Some(UnitPrice::from_str("100").unwrap()));
        assert!(
            SavePurchaseOrderLinePatch::resolve_all(&patches, &lines).is_err(),
            "旧行身份不得套用到新草稿"
        );
    }

    #[test]
    fn reopen_rejects_all_non_approval_previous_states_and_preserves_target() {
        for status in [
            PurchaseOrderStatus::Draft,
            PurchaseOrderStatus::PendingFinanceReview,
            PurchaseOrderStatus::Effective,
            PurchaseOrderStatus::PartiallyExecuted,
            PurchaseOrderStatus::Completed,
            PurchaseOrderStatus::Voided,
        ] {
            let (mut previous, mut cancelled, source, lines) = fixture();
            previous.stable.status = status;
            let before = cancelled.clone();
            let error = reopen(&mut cancelled, &previous, &source, &lines).err().unwrap();
            assert_eq!(error.to_string(), "采购单不是可恢复草稿的审批提交");
            assert_eq!(cancelled, before);
        }
    }

    #[test]
    fn reopen_requires_exact_cancelled_aggregate_version_and_content() {
        let mutations: [fn(&mut PurchaseOrder); 6] = [
            |order| order.base.version += 1,
            |order| order.approval_subject_version += 1,
            |order| order.current_submission_id = Some("foreign-submission".into()),
            |order| order.stable.status = PurchaseOrderStatus::InApproval,
            |order| order.purchase_no = "PO-rewritten".into(),
            |order| order.owner_user_id = Some("different-owner".into()),
        ];
        for mutate in mutations {
            let (previous, mut cancelled, source, lines) = fixture();
            mutate(&mut cancelled);
            let before = cancelled.clone();
            let error = reopen(&mut cancelled, &previous, &source, &lines).err().unwrap();
            assert_eq!(error.to_string(), "采购单取消版本或内容已变化");
            assert_eq!(cancelled, before);
        }
    }

    #[test]
    fn reopen_rejects_unfrozen_approved_superseded_and_foreign_source_snapshots() {
        let mutations: [fn(&mut PurchaseOrderSubmission); 9] = [
            |source| source.status = SubmissionStatus::Draft,
            |source| source.status = SubmissionStatus::Approved,
            |source| source.status = SubmissionStatus::Superseded,
            |source| source.purchase_order_id = PurchaseOrderId::new("foreign-order"),
            |source| source.base.id = "foreign-submission".into(),
            |source| source.supplier_id = SupplierAccountId::new("foreign-supplier"),
            |source| source.payment_term_snapshot.payment_term_code = "CASH_ON_APPROVAL".into(),
            |source| source.submitted_at = None,
            |source| source.submitted_by = None,
        ];
        for mutate in mutations {
            let (previous, mut cancelled, mut source, lines) = fixture();
            mutate(&mut source);
            let before = cancelled.clone();
            let error = reopen(&mut cancelled, &previous, &source, &lines).err().unwrap();
            assert_eq!(error.to_string(), "采购取消来源不是当前冻结提交");
            assert_eq!(cancelled, before);
        }
    }

    #[test]
    fn reopen_rejects_reused_missing_duplicate_or_deleted_line_identities() {
        for ids in [vec!["draft-item"], vec!["draft-item", "draft-item"], vec!["old-item", "draft-logistics"]]
        {
            let (previous, mut cancelled, source, lines) = fixture();
            let before = cancelled.clone();
            let ids = ids.into_iter().map(PurchaseOrderSubmissionLineId::new).collect::<Vec<_>>();
            let error = cancelled
                .reopen_cancelled_draft(
                    &previous,
                    &source,
                    &lines,
                    PurchaseOrderSubmissionId::new("draft-2"),
                    &ids,
                )
                .err()
                .unwrap();
            assert_eq!(error.to_string(), "采购取消草稿必须使用独立的完整头行身份");
            assert_eq!(cancelled, before);
        }
        let (previous, mut cancelled, source, mut lines) = fixture();
        let before = cancelled.clone();
        lines[0].base.deleted_at = 1;
        assert_eq!(
            reopen(&mut cancelled, &previous, &source, &lines).err().unwrap().to_string(),
            "采购取消草稿必须使用独立的完整头行身份"
        );
        assert_eq!(cancelled, before);
        let (_, _, _, lines) = fixture();
        let error = cancelled
            .reopen_cancelled_draft(
                &previous,
                &source,
                &lines,
                PurchaseOrderSubmissionId::new("formal-1"),
                &[
                    PurchaseOrderSubmissionLineId::new("draft-item"),
                    PurchaseOrderSubmissionLineId::new("draft-logistics"),
                ],
            )
            .err()
            .unwrap();
        assert_eq!(error.to_string(), "采购取消草稿必须使用独立的完整头行身份");
        assert_eq!(cancelled, before);
    }

    #[test]
    fn reopen_rejects_foreign_or_inconsistent_lines_without_switching_pointer() {
        for foreign in [true, false] {
            let (previous, mut cancelled, source, mut lines) = fixture();
            if foreign {
                lines[0].purchase_order_submission_id = PurchaseOrderSubmissionId::new("foreign");
            } else {
                lines[0].gross_amount = Amount::from_str("201").unwrap();
            }
            let before = cancelled.clone();
            let error = reopen(&mut cancelled, &previous, &source, &lines).err().unwrap();
            assert_eq!(
                error.to_string(),
                if foreign {
                    "采购提交明细不属于当前提交"
                } else {
                    "采购提交表头金额与冻结明细汇总不一致"
                }
            );
            assert_eq!(cancelled, before);
        }
    }

    #[test]
    fn reopen_requires_live_never_formalized_approval_source() {
        let mutations: [fn(&mut PurchaseOrder); 3] = [
            |order| order.base.deleted_at = 1,
            |order| order.stable.current_revision_id = Some("effective-revision".into()),
            |order| order.approval_subject_version = 0,
        ];
        for mutate in mutations {
            let (mut previous, mut cancelled, source, lines) = fixture();
            mutate(&mut previous);
            let before = cancelled.clone();
            let error = reopen(&mut cancelled, &previous, &source, &lines).err().unwrap();
            assert_eq!(error.to_string(), "采购单不是可恢复草稿的审批提交");
            assert_eq!(cancelled, before);
        }
        let (previous, mut cancelled, mut source, lines) = fixture();
        source.base.deleted_at = 1;
        let before = cancelled.clone();
        assert_eq!(
            reopen(&mut cancelled, &previous, &source, &lines).err().unwrap().to_string(),
            "采购单不是可恢复草稿的审批提交"
        );
        assert_eq!(cancelled, before);
    }

    #[test]
    fn reopen_rejects_empty_or_invalid_full_lines_without_partial_target_mutation() {
        let (previous, mut cancelled, mut source, _) = fixture();
        source.gross_amount = Amount::from_str("0").unwrap();
        source.net_amount = source.gross_amount;
        source.tax_amount = source.gross_amount;
        let before = cancelled.clone();
        let error = cancelled
            .reopen_cancelled_draft(&previous, &source, &[], PurchaseOrderSubmissionId::new("draft-2"), &[])
            .err()
            .unwrap();
        assert_eq!(error.to_string(), "采购取消草稿必须使用独立的完整头行身份");
        assert_eq!(cancelled, before);

        let (previous, mut cancelled, source, mut lines) = fixture();
        lines[0].specification_snapshot = None;
        let before = cancelled.clone();
        assert!(reopen(&mut cancelled, &previous, &source, &lines).is_err());
        assert_eq!(cancelled, before);
    }
}
