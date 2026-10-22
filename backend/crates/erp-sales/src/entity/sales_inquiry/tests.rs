use erp_core::money::{Amount, Quantity};
use erp_core::region::RegionCode;

use super::*;
use crate::dto::sales_inquiry::{InquiryProcurementView, InquirySalesView};

fn now() -> Instant {
    Instant::from_unix_secs(1000)
}
fn later() -> Instant {
    Instant::from_unix_secs(5000)
}
fn qty(value: &str) -> Quantity {
    value.parse().unwrap()
}
fn amount(value: &str) -> Amount {
    value.parse().unwrap()
}

fn demand() -> InquiryRequirementData {
    InquiryRequirementData {
        lines: vec![InquiryRequirementLine {
            line_id: "line".into(),
            sku_id: "sku".into(),
            sku_revision_id: "sku-rev".into(),
            sku_code: "001".into(),
            product_name: "测试商品".into(),
            specification: "盒".into(),
            unit_code: "盒".into(),
            quantity_scale: 2,
            quantity: qty("10"),
            fulfillment_kind: InquiryFulfillmentKind::Physical,
            delivery_due_at: later(),
            delivery_terms: "按期配送".into(),
            sales_reference_price_gross: Some("99".parse().unwrap()),
            allocations: vec![InquiryDeliveryAllocation {
                allocation_id: "address".into(),
                recipient: " 张三 ".into(),
                phone: " 0013800000000 ".into(),
                region_code: RegionCode::parse("310115").unwrap(),
                region_name: String::new(),
                address: " 测试路1号 ".into(),
                quantity: qty("10"),
            }],
        }],
        note: "公开交付说明".into(),
    }
}

fn inquiry() -> SalesInquiry {
    let mut inquiry = SalesInquiry::create(
        "inquiry".into(),
        "IQ1".into(),
        InquiryCreateData {
            customer_id: "customer".into(),
            customer_name: "客户".into(),
            sales_owner_user_id: "sales".into(),
            business_org_unit_id: "org".into(),
            deadline_at: later(),
            copied_from_inquiry_id: None,
            draft: demand(),
        },
    )
    .unwrap();
    inquiry.base = BaseModel { id: "inquiry".into(), ..BaseModel::fake() };
    inquiry
}

fn feedback(quantity: &str, conclusion: InquirySupplyConclusion) -> InquiryFeedbackData {
    let prices = if quantity == "0" {
        vec![]
    } else {
        vec![InquirySupplyPrice {
            price_id: "price".into(),
            unit_price_gross: Some("12.34".parse().unwrap()),
            tax_rate: Some("0.13".parse().unwrap()),
            valid_until: later(),
        }]
    };
    InquiryFeedbackData {
        lines: vec![InquiryFeedbackLine {
            line_id: "line".into(),
            conclusion,
            available_quantity: qty(quantity),
            prices,
            public_note: "剩余待重新确认".into(),
        }],
        fees: vec![InquiryFee {
            fee_id: "fee".into(),
            name: "运费".into(),
            amount_gross: amount("100"),
            tax_rate: "0.06".parse().unwrap(),
            line_ids: vec!["line".into()],
            valid_until: later(),
            public_note: "物流成本".into(),
        }],
    }
}

fn accepted() -> SalesInquiry {
    let mut inquiry = inquiry();
    inquiry.submit(1, "sales", String::new(), now()).unwrap();
    inquiry.publish(1, "buyer", feedback("10", InquirySupplyConclusion::Available), now()).unwrap();
    inquiry.accept(1, "sales", 1, now()).unwrap();
    inquiry
}

fn selected(quantity: &str) -> Vec<InquiryQuantitySelection> {
    vec![InquiryQuantitySelection {
        line_id: "line".into(),
        allocation_id: Some("address".into()),
        price_id: Some("price".into()),
        quantity: qty(quantity),
    }]
}

fn fee_reservation(id: &str, value: &str) -> InquiryFeeReservation {
    InquiryFeeReservation {
        reservation_id: id.into(),
        allocation_id: "allocation".into(),
        fee_id: "fee".into(),
        sales_order_id: "sales-order".into(),
        feedback_round: 1,
        amount: amount(value),
        state: InquiryReservationState::Reserved,
        source_document_id: "purchase".into(),
        source_line_id: id.into(),
    }
}

#[test]
fn physical_address_quantity_and_unit_precision_are_strict() {
    let mut inquiry = inquiry();
    assert_eq!(inquiry.draft.lines[0].allocations[0].phone, "0013800000000");
    assert_eq!(inquiry.draft.lines[0].allocations[0].region_name, "上海市浦东新区");
    inquiry.draft.lines[0].allocations.clear();
    assert!(inquiry.submit(1, "sales", String::new(), now()).is_err());
    let mut data = demand();
    data.lines[0].allocations[0].quantity = qty("9.99");
    assert!(data.normalize(true).is_err());
    let mut data = demand();
    data.lines[0].quantity = qty("10.001");
    assert!(data.normalize(true).is_err());
    let mut data = demand();
    data.lines[0].allocations = (0..101)
        .map(|index| InquiryDeliveryAllocation {
            allocation_id: format!("a{index}"),
            ..data.lines[0].allocations[0].clone()
        })
        .collect();
    assert!(data.normalize(true).is_err());
    let mut service = demand();
    service.lines[0].fulfillment_kind = InquiryFulfillmentKind::Service;
    service.lines[0].allocations.clear();
    service.normalize(true).unwrap();
}

#[test]
fn draft_and_submission_preserve_excess_allocation_quantities() {
    for quantity in ["10", "30"] {
        let mut inquiry = inquiry();
        let mut data = inquiry.draft.clone();
        data.lines[0].allocations[0].quantity = qty(quantity);
        inquiry.save_draft(1, data.clone(), None).unwrap();
        assert_eq!(inquiry.draft, data);
        assert_eq!(inquiry.draft.lines[0].quantity, qty("10"));
        inquiry.submit(1, "sales", String::new(), now()).unwrap();
        assert_eq!(inquiry.status, InquiryStatus::ProcurementProcessing);
        assert_eq!(inquiry.requirements[0].data, data);
    }
}

#[test]
fn draft_saves_short_allocation_but_submission_requires_full_coverage() {
    let mut inquiry = inquiry();
    let mut data = inquiry.draft.clone();
    data.lines[0].allocations[0].quantity = qty("6");
    inquiry.save_draft(1, data.clone(), None).unwrap();
    let error = inquiry.submit(1, "sales", String::new(), now()).unwrap_err();
    assert!(matches!(error, Error::ValidationError(message) if message == "地址数量合计不得少于需求数量"));
    assert_eq!(inquiry.status, InquiryStatus::Draft);
    assert!(inquiry.requirements.is_empty());
    assert_eq!(inquiry.draft, data);
    data.lines[0].allocations[0].quantity = qty("10");
    inquiry.save_draft(1, data, None).unwrap();
    inquiry.submit(1, "sales", String::new(), now()).unwrap();
    assert_eq!(inquiry.status, InquiryStatus::ProcurementProcessing);
}

#[test]
fn draft_still_rejects_invalid_allocation_quantities_without_changing_saved_data() {
    for quantity in ["0", "10.001"] {
        let mut inquiry = inquiry();
        let original = inquiry.draft.clone();
        let mut data = original.clone();
        data.lines[0].allocations[0].quantity = qty(quantity);
        let error = inquiry.save_draft(1, data, None).unwrap_err();
        assert!(
            matches!(error, Error::ValidationError(message) if message == "地址数量必须为正且符合单位精度")
        );
        assert_eq!(inquiry.draft, original);
    }
}

#[test]
fn three_feedback_rounds_preserve_accepted_and_submitted_history() {
    let mut inquiry = accepted();
    let original = inquiry.feedbacks[0].clone();
    for round in 2..=3 {
        inquiry
            .reinquire(
                1,
                "sales",
                InquiryReinquiryData {
                    reason: "重新议价".into(),
                    line_ids: vec!["line".into()],
                    fee_ids: vec![],
                    concerns: "价格".into(),
                    requirement: None,
                },
                now(),
            )
            .unwrap();
        assert!(inquiry.reserve_quantities("blocked", "sales", round - 1, selected("1"), now()).is_err());
        inquiry.publish(1, "buyer", feedback("10", InquirySupplyConclusion::Available), now()).unwrap();
        inquiry.accept(1, "sales", round, now()).unwrap();
    }
    assert_eq!(inquiry.requirement_version, 1);
    assert_eq!(inquiry.requirements.len(), 1);
    assert_eq!(inquiry.feedbacks.len(), 3);
    assert_eq!(inquiry.acceptances.len(), 3);
    assert_eq!(inquiry.feedbacks[0], original);
    assert_eq!(inquiry.reinquiries.len(), 2);
}

#[test]
fn only_final_complete_feedback_can_be_published() {
    let mut inquiry = inquiry();
    inquiry.submit(1, "sales", String::new(), now()).unwrap();
    let before = inquiry.clone();
    assert!(inquiry.publish(1, "buyer", feedback("10", InquirySupplyConclusion::Pending), now()).is_err());
    assert_eq!(inquiry, before);
    let mut incomplete = feedback("5", InquirySupplyConclusion::PartiallyAvailable);
    incomplete.lines[0].prices[0].tax_rate = None;
    assert!(inquiry.publish(1, "buyer", incomplete, now()).is_err());
    inquiry.publish(1, "buyer", feedback("5", InquirySupplyConclusion::PartiallyAvailable), now()).unwrap();
    inquiry.accept(1, "sales", 1, now()).unwrap();
    assert!(inquiry.reserve_quantities("too-many", "order", 1, selected("6"), now()).is_err());
    inquiry.reserve_quantities("partial", "order", 1, selected("5"), now()).unwrap();
}

#[test]
fn all_unavailable_feedback_is_publishable_and_close_preserves_history() {
    let mut inquiry = inquiry();
    inquiry.submit(1, "sales", String::new(), now()).unwrap();
    inquiry.publish(1, "buyer", feedback("0", InquirySupplyConclusion::Unavailable), now()).unwrap();
    inquiry.accept(1, "sales", 1, now()).unwrap();
    assert!(inquiry.reserve_quantities("r", "order", 1, selected("1"), now()).is_err());
    inquiry.close(1, "sales", "客户暂不采购".into(), now()).unwrap();
    assert_eq!(inquiry.feedbacks.len(), 1);
    assert_eq!(inquiry.status, InquiryStatus::Closed);
    assert!(inquiry.ensure_processing(1).is_err());
}

#[test]
fn quantity_ledger_survives_requery_and_rejects_new_version_reset() {
    let mut inquiry = accepted();
    inquiry.reserve_quantities("r1", "order", 1, selected("7"), now()).unwrap();
    let before = inquiry.clone();
    inquiry.reserve_quantities("r1", "order", 1, selected("7"), now()).unwrap();
    assert_eq!(inquiry, before);
    assert!(inquiry.reserve_quantities("r1", "order", 1, selected("6"), now()).is_err());
    assert!(inquiry.reserve_quantities("r2", "order2", 1, selected("4"), now()).is_err());
    let mut smaller = inquiry.draft.clone();
    smaller.lines[0].quantity = qty("6");
    smaller.lines[0].allocations[0].quantity = qty("6");
    assert!(
        inquiry
            .reinquire(
                1,
                "sales",
                InquiryReinquiryData {
                    reason: "减少需求".into(),
                    line_ids: vec![],
                    fee_ids: vec![],
                    concerns: String::new(),
                    requirement: Some(smaller)
                },
                now()
            )
            .is_err()
    );
    assert_eq!(inquiry, before);
    inquiry.close(1, "sales", "关闭".into(), now()).unwrap();
    inquiry.reconcile_quantities("r1", selected("5"), Instant::from_unix_secs(6000)).unwrap();
    inquiry.consume_quantities("r1").unwrap();
    inquiry.consume_quantities("r1").unwrap();
    assert_eq!(inquiry.used_quantity("line", None, None).unwrap(), qty("5").to_decimal());
    assert!(inquiry.release_quantities("r1").is_err());
}

#[test]
fn occupied_stable_sku_address_and_fee_identity_cannot_be_rewritten() {
    let inquiry = accepted();
    let mut data = inquiry.draft.clone();
    data.lines[0].sku_id = "replacement".into();
    assert!(inquiry.validate_requirement_history(&data).is_err());
    let mut data = inquiry.draft.clone();
    data.lines[0].allocations[0].address = "另一个地址".into();
    assert!(inquiry.validate_requirement_history(&data).is_err());
    let mut inquiry = inquiry;
    inquiry.allocate_fee("allocation", "sales-order", 1, "fee", amount("100"), now()).unwrap();
    inquiry.reserve_fee(fee_reservation("f1", "80"), now()).unwrap();
    inquiry
        .reinquire(
            1,
            "sales",
            InquiryReinquiryData {
                reason: "减少费用".into(),
                line_ids: vec![],
                fee_ids: vec!["fee".into()],
                concerns: String::new(),
                requirement: None,
            },
            now(),
        )
        .unwrap();
    let mut lower = feedback("10", InquirySupplyConclusion::Available);
    lower.fees[0].amount_gross = amount("70");
    assert!(inquiry.publish(1, "buyer", lower, now()).is_err());
}

#[test]
fn fees_cannot_be_duplicated_or_release_registered_facts() {
    let mut inquiry = accepted();
    inquiry.allocate_fee("allocation", "sales-order", 1, "fee", amount("60"), now()).unwrap();
    assert!(inquiry.allocate_fee("other", "sales2", 1, "fee", amount("50"), now()).is_err());
    inquiry.reserve_fee(fee_reservation("f1", "40"), now()).unwrap();
    let before = inquiry.clone();
    assert!(inquiry.reserve_fee(fee_reservation("f2", "30"), now()).is_err());
    assert_eq!(inquiry, before);
    inquiry.register_fee("f1").unwrap();
    inquiry.register_fee("f1").unwrap();
    assert!(inquiry.release_fee("f1").is_err());
    assert!(inquiry.adjust_fee("f1", amount("10"), now()).is_err());
}

#[test]
fn extra_costs_are_independent_and_zero_cannot_be_handled() {
    let mut inquiry = accepted();
    assert_eq!(inquiry.feedbacks[0].data.extra_cost_total().unwrap(), amount("100"));
    inquiry.feedbacks[0].data.fees[0].amount_gross = amount("0");
    assert!(inquiry.ensure_fee_available(1, "fee", now()).is_err());
}

#[test]
fn new_accepted_fee_can_resize_existing_allocation_and_unhandled_reservation() {
    let mut inquiry = accepted();
    inquiry.allocate_fee("allocation", "sales-order", 1, "fee", amount("100"), now()).unwrap();
    inquiry.reserve_fee(fee_reservation("f1", "80"), now()).unwrap();
    inquiry
        .reinquire(
            1,
            "sales",
            InquiryReinquiryData {
                reason: "涨价确认".into(),
                line_ids: vec![],
                fee_ids: vec!["fee".into()],
                concerns: String::new(),
                requirement: None,
            },
            now(),
        )
        .unwrap();
    let mut raised = feedback("10", InquirySupplyConclusion::Available);
    raised.fees[0].amount_gross = amount("150");
    inquiry.publish(1, "buyer", raised, now()).unwrap();
    inquiry.accept(1, "sales", 2, now()).unwrap();
    inquiry.allocate_fee("allocation", "sales-order", 2, "fee", amount("150"), now()).unwrap();
    inquiry.adjust_fee_for_round("f1", amount("140"), 2, now()).unwrap();
    assert_eq!(inquiry.fee_ledger[0].feedback_round, 2);
    assert_eq!(inquiry.fee_allocation_history[0].amount, amount("100"));
    assert!(inquiry.allocate_fee("allocation", "sales-order", 2, "fee", amount("130"), now()).is_err());
}

#[test]
fn current_task_work_is_required_and_reassignment_does_not_revive_old_work() {
    let mut inquiry = inquiry();
    inquiry.submit(1, "sales", String::new(), now()).unwrap();
    inquiry
        .save_feedback_work(
            1,
            "buyer1",
            vec!["line".into()],
            feedback("10", InquirySupplyConclusion::Available),
            now(),
        )
        .unwrap();
    assert!(inquiry.merge_feedback_work(&[("buyer1".into(), vec!["line".into()])]).is_ok());
    inquiry
        .reassign_work(
            1,
            "manager",
            InquiryReassignmentData {
                previous_owner: "buyer1".into(),
                target_owner: "buyer2".into(),
                line_ids: vec!["line".into()],
                reason: "轮岗".into(),
            },
            now(),
        )
        .unwrap();
    assert!(inquiry.merge_feedback_work(&[("buyer1".into(), vec!["line".into()])]).is_err());
    assert!(inquiry.merge_feedback_work(&[("buyer2".into(), vec!["line".into()])]).is_err());
    inquiry
        .save_feedback_work(
            1,
            "buyer2",
            vec!["line".into()],
            feedback("10", InquirySupplyConclusion::Available),
            now(),
        )
        .unwrap();
    assert!(inquiry.merge_feedback_work(&[("buyer2".into(), vec!["line".into()])]).is_ok());
}

#[test]
fn independent_sales_and_procurement_projections_enforce_field_whitelists() {
    let mut inquiry = accepted();
    let mut hidden = feedback("10", InquirySupplyConclusion::Available);
    hidden.lines[0].public_note = "unpublished-private-draft".into();
    inquiry.feedback_work.push(InquiryFeedbackWork {
        requirement_version: 1,
        actor_id: "buyer".into(),
        line_ids: vec!["line".into()],
        data: hidden,
        saved_at: now(),
        frozen: false,
    });
    let sales = serde_json::to_value(InquirySalesView::from_inquiry(&inquiry)).unwrap();
    assert!(sales.get("feedback_work").is_none());
    assert!(!sales.to_string().contains("unpublished-private-draft"));
    let procurement =
        serde_json::to_value(InquiryProcurementView::from_scoped(&inquiry, &["line".into()])).unwrap();
    assert!(procurement["draft"]["lines"][0].get("sales_reference_price_gross").is_none());
    assert!(procurement["requirements"][0]["data"]["lines"][0].get("sales_reference_price_gross").is_none());
    let empty = InquiryProcurementView::from_scoped(&inquiry, &[]);
    assert!(empty.draft.lines.is_empty());
    assert!(empty.feedbacks[0].data.lines.is_empty());
    assert!(empty.feedbacks[0].data.fees.is_empty());
}

#[test]
fn stale_version_and_invalid_command_replay_fail_without_changes() {
    let mut inquiry = inquiry();
    let before = inquiry.clone();
    assert!(inquiry.submit(99, "sales", String::new(), now()).is_err());
    assert_eq!(inquiry, before);
    let receipt = InquiryCommandReceipt::new(
        "cmd".into(),
        "sales",
        "key",
        "inquiry.create",
        "a".repeat(64),
        InquiryCommandResult::Created { inquiry_id: "inquiry".into() },
    )
    .unwrap();
    assert!(receipt.matches("sales", "key", "inquiry.create", &"a".repeat(64)).is_ok());
    assert!(receipt.matches("sales", "key", "inquiry.create", &"b".repeat(64)).is_err());
    assert!(receipt.matches("other", "key", "inquiry.create", &"a".repeat(64)).is_err());
    assert!(!serde_json::to_string(&receipt).unwrap().contains("\"key\""));
}

#[test]
fn released_fee_can_reenter_a_valid_explicit_round_but_handled_fact_is_frozen() {
    let mut inquiry = accepted();
    inquiry.allocate_fee("allocation", "sales-order", 1, "fee", amount("100"), now()).unwrap();
    inquiry.reserve_fee(fee_reservation("f1", "50"), now()).unwrap();
    inquiry.adjust_fee("f1", amount("0"), Instant::from_unix_secs(6000)).unwrap();
    assert_eq!(inquiry.fee_ledger[0].state, InquiryReservationState::Released);
    inquiry.adjust_fee_for_round("f1", amount("50"), 1, now()).unwrap();
    assert_eq!(inquiry.fee_ledger[0].state, InquiryReservationState::Reserved);
    assert_eq!(inquiry.fee_available_amount("fee", "allocation", 1, now()).unwrap(), amount("50"));
    inquiry.register_fee("f1").unwrap();
    inquiry.adjust_fee_for_round("f1", amount("50"), 1, Instant::from_unix_secs(6000)).unwrap();
    assert!(inquiry.adjust_fee_for_round("f1", amount("50"), 2, now()).is_err());
    assert!(inquiry.adjust_fee_for_round("f1", amount("10"), 1, now()).is_err());
}

#[test]
fn fee_only_requery_keeps_unaffected_goods_transfer_available() {
    let mut inquiry = accepted();
    inquiry
        .reinquire(
            1,
            "sales",
            InquiryReinquiryData {
                reason: "只询运费".into(),
                line_ids: vec![],
                fee_ids: vec!["fee".into()],
                concerns: String::new(),
                requirement: None,
            },
            now(),
        )
        .unwrap();
    inquiry.reserve_quantities("r", "order", 1, selected("2"), now()).unwrap();
    assert!(inquiry.ensure_fee_available(1, "fee", now()).is_err());
}

#[test]
fn unknown_receipt_schema_deleted_receipts_and_corrupt_results_fail_closed() {
    let receipt = InquiryCommandReceipt::new(
        "cmd".into(),
        "sales",
        "key",
        "create",
        "a".repeat(64),
        InquiryCommandResult::Created { inquiry_id: "inquiry".into() },
    )
    .unwrap();
    let mut corrupted = receipt.clone();
    corrupted.schema_version = 2;
    assert!(corrupted.validate().is_err());
    assert!(corrupted.matches("sales", "key", "create", &"a".repeat(64)).is_err());
    let mut deleted = receipt.clone();
    deleted.base.deleted_at = 1;
    assert!(deleted.validate().is_err());
    let mut corrupted = receipt;
    corrupted.result = InquiryCommandResult::Created { inquiry_id: String::new() };
    assert!(corrupted.validate().is_err());
}

#[test]
fn each_product_rejects_multiple_prices() {
    let mut inquiry = inquiry();
    inquiry.submit(1, "sales", String::new(), now()).unwrap();
    let mut data = feedback("10", InquirySupplyConclusion::Available);
    let mut second = data.lines[0].prices[0].clone();
    second.price_id = "second-price".into();
    data.lines[0].prices.push(second);
    assert!(inquiry.publish(1, "buyer", data, now()).is_err());
}

#[test]
fn reducing_old_draft_still_respects_frozen_unit_precision() {
    let mut inquiry = accepted();
    inquiry.requirements[0].data.lines[0].quantity_scale = 0;
    inquiry.reserve_quantities("r", "order", 1, selected("5"), now()).unwrap();
    inquiry.close(1, "sales", "关闭".into(), now()).unwrap();
    assert!(inquiry.reconcile_quantities("r", selected("4.5"), Instant::from_unix_secs(6000)).is_err());
    inquiry.reconcile_quantities("r", selected("4"), Instant::from_unix_secs(6000)).unwrap();
}

#[test]
fn shared_fee_owner_is_explicit_and_moves_with_responsibility() {
    let mut inquiry = inquiry();
    inquiry.assign_shared_fee_owner(None, &["buyer1".into(), "buyer2".into()]).unwrap();
    assert_eq!(inquiry.shared_fee_owner_user_id.as_deref(), Some("buyer1"));
    inquiry.assign_shared_fee_owner(None, &["buyer2".into(), "buyer1".into()]).unwrap();
    assert_eq!(inquiry.shared_fee_owner_user_id.as_deref(), Some("buyer1"));
    inquiry.assign_shared_fee_owner(None, &["buyer2".into()]).unwrap();
    assert_eq!(inquiry.shared_fee_owner_user_id.as_deref(), Some("buyer2"));
    assert!(inquiry.assign_shared_fee_owner(None, &[]).is_err());
    assert_eq!(inquiry.shared_fee_owner_user_id.as_deref(), Some("buyer2"));
    assert!(inquiry.assign_shared_fee_owner(Some("outside".into()), &["buyer1".into()]).is_err());
    inquiry.assign_shared_fee_owner(None, &["buyer1".into()]).unwrap();
    assert_eq!(inquiry.shared_fee_owner_user_id.as_deref(), Some("buyer1"));
    inquiry.submit(1, "sales", String::new(), now()).unwrap();
    let mut whole_fee = feedback("10", InquirySupplyConclusion::Available);
    whole_fee.fees[0].line_ids.clear();
    inquiry.save_feedback_work(1, "buyer1", vec!["line".into()], whole_fee.clone(), now()).unwrap();
    assert!(inquiry.merge_feedback_work(&[("buyer1".into(), vec!["line".into()])]).is_ok());
    inquiry.shared_fee_owner_user_id = Some("buyer2".into());
    assert!(inquiry.merge_feedback_work(&[("buyer1".into(), vec!["line".into()])]).is_err());
    inquiry.shared_fee_owner_user_id = Some("buyer1".into());
    inquiry
        .reassign_work(
            1,
            "manager",
            InquiryReassignmentData {
                previous_owner: "buyer1".into(),
                target_owner: "buyer2".into(),
                line_ids: vec!["line".into()],
                reason: "改派".into(),
            },
            now(),
        )
        .unwrap();
    assert_eq!(inquiry.shared_fee_owner_user_id.as_deref(), Some("buyer2"));
}

#[test]
fn assigned_and_legacy_inquiries_allow_processing_without_acceptance() {
    for legacy in [false, true] {
        let mut inquiry = inquiry();
        inquiry.submit(1, "sales", String::new(), now()).unwrap();
        assert_eq!(inquiry.status, InquiryStatus::ProcurementProcessing);
        if legacy {
            inquiry.status = InquiryStatus::AwaitingProcurement;
        }
        inquiry.ensure_processing(1).unwrap();
        assert!(inquiry.ensure_processing(2).is_err());
        let data = feedback("10", InquirySupplyConclusion::Available);
        let original = inquiry.clone();
        assert!(inquiry.save_feedback_work(2, "buyer", vec!["line".into()], data.clone(), now()).is_err());
        assert_eq!(inquiry, original);
        assert!(inquiry.save_feedback_work(1, "buyer", vec!["outside".into()], data.clone(), now()).is_err());
        assert_eq!(inquiry, original);
        inquiry.save_feedback_work(1, "buyer", vec!["line".into()], data, now()).unwrap();
        assert_eq!(inquiry.feedback_work.len(), 1);
        let merged = inquiry.merge_feedback_work(&[("buyer".into(), vec!["line".into()])]).unwrap();
        inquiry.publish(1, "buyer", merged, now()).unwrap();
        assert_eq!(inquiry.status, InquiryStatus::AwaitingSales);
        inquiry
            .reinquire(
                1,
                "sales",
                InquiryReinquiryData {
                    reason: "调整交期".into(),
                    line_ids: vec![],
                    fee_ids: vec![],
                    concerns: String::new(),
                    requirement: None,
                },
                now(),
            )
            .unwrap();
        assert_eq!(inquiry.status, InquiryStatus::ProcurementProcessing);
        inquiry.request_supplement(1, "buyer", "补充说明".into(), now()).unwrap();
        assert_eq!(inquiry.status, InquiryStatus::AwaitingSalesSupplement);
        assert!(inquiry.ensure_processing(1).is_err());
        inquiry.submit(1, "sales", String::new(), now()).unwrap();
        assert_eq!(inquiry.status, InquiryStatus::ProcurementProcessing);
    }
}

#[test]
fn mutation_state_matrix_matches_the_contract() {
    for state in [
        InquiryStatus::Draft,
        InquiryStatus::AwaitingProcurement,
        InquiryStatus::ProcurementProcessing,
        InquiryStatus::AwaitingSalesSupplement,
        InquiryStatus::AwaitingSales,
        InquiryStatus::Confirmed,
        InquiryStatus::Closed,
    ] {
        let mut initial = accepted();
        initial.status = state;
        let editable = matches!(state, InquiryStatus::Draft | InquiryStatus::AwaitingSalesSupplement);
        assert_eq!(
            initial.clone().save_draft(1, initial.draft.clone(), Some(now())).is_ok(),
            editable,
            "save {state:?}"
        );
        assert_eq!(
            initial.clone().submit(1, "sales", "改版".into(), now()).is_ok(),
            editable,
            "submit {state:?}"
        );
        assert_eq!(
            initial.ensure_processing(1).is_ok(),
            matches!(state, InquiryStatus::AwaitingProcurement | InquiryStatus::ProcurementProcessing),
            "processing {state:?}"
        );
        assert_eq!(
            initial.clone().request_supplement(1, "buyer", "补充".into(), now()).is_ok(),
            matches!(state, InquiryStatus::AwaitingProcurement | InquiryStatus::ProcurementProcessing),
            "supplement {state:?}"
        );
        assert_eq!(
            initial
                .clone()
                .publish(1, "buyer", feedback("10", InquirySupplyConclusion::Available), now())
                .is_ok(),
            matches!(state, InquiryStatus::AwaitingProcurement | InquiryStatus::ProcurementProcessing),
            "publish {state:?}"
        );
        assert_eq!(
            initial.clone().accept(1, "sales", 1, now()).is_ok(),
            state == InquiryStatus::AwaitingSales,
            "accept {state:?}"
        );
        assert_eq!(
            initial
                .clone()
                .reinquire(
                    1,
                    "sales",
                    InquiryReinquiryData {
                        reason: "重询".into(),
                        line_ids: vec![],
                        fee_ids: vec![],
                        concerns: String::new(),
                        requirement: None
                    },
                    now()
                )
                .is_ok(),
            matches!(state, InquiryStatus::AwaitingSales | InquiryStatus::Confirmed),
            "reinquire {state:?}"
        );
        assert_eq!(
            initial.clone().close(1, "sales", "关闭".into(), now()).is_ok(),
            state != InquiryStatus::Closed,
            "close {state:?}"
        );
    }
}

#[test]
fn public_feedback_input_rejects_internal_supplier_fields() {
    let mut payload = serde_json::to_value(feedback("10", InquirySupplyConclusion::Available)).unwrap();
    payload["lines"][0]["supplier_id"] = "secret-supplier".into();
    assert!(serde_json::from_value::<InquiryFeedbackData>(payload).is_err());
    let mut payload = serde_json::to_value(feedback("10", InquirySupplyConclusion::Available)).unwrap();
    payload["fees"][0]["payee_id"] = "secret-payee".into();
    assert!(serde_json::from_value::<InquiryFeedbackData>(payload).is_err());
}

#[test]
fn inquiry_quote_scope_requires_physical_addresses_and_demand_bounded_supply() {
    let line = &demand().lines[0];
    line.validate_quote_scope(qty("5"), &["address".into()]).unwrap();
    assert!(line.validate_quote_scope(qty("5"), &[]).is_err());
    assert!(line.validate_quote_scope(qty("11"), &["address".into()]).is_err());
    assert!(line.validate_quote_scope(qty("5"), &["other-address".into()]).is_err());
    assert!(line.validate_quote_scope(qty("5.001"), &["address".into()]).is_err());
    let mut virtual_line = line.clone();
    virtual_line.fulfillment_kind = InquiryFulfillmentKind::Virtual;
    virtual_line.allocations.clear();
    virtual_line.validate_quote_scope(qty("5"), &[]).unwrap();
    virtual_line.validate_supply_quantity(qty("20")).unwrap();
}

fn two_line_accepted_inquiry() -> (SalesInquiry, InquiryFeedbackData) {
    let mut inquiry = inquiry();
    let mut second_demand = inquiry.draft.lines[0].clone();
    second_demand.line_id = "line-b".into();
    second_demand.sku_id = "sku-b".into();
    second_demand.sku_revision_id = "sku-b-rev".into();
    second_demand.sku_code = "002".into();
    second_demand.allocations[0].allocation_id = "address-b".into();
    inquiry.draft.lines.push(second_demand);
    inquiry.submit(1, "sales", String::new(), now()).unwrap();
    let mut complete = feedback("10", InquirySupplyConclusion::Available);
    let mut second_line = complete.lines[0].clone();
    second_line.line_id = "line-b".into();
    second_line.prices[0].price_id = "price-b".into();
    complete.lines.push(second_line);
    let mut second_fee = complete.fees[0].clone();
    second_fee.fee_id = "fee-b".into();
    second_fee.line_ids = vec!["line-b".into()];
    complete.fees.push(second_fee);
    inquiry.publish(1, "buyer", complete.clone(), now()).unwrap();
    inquiry.accept(1, "sales", 1, now()).unwrap();
    (inquiry, complete)
}

fn selected_b(quantity: &str) -> Vec<InquiryQuantitySelection> {
    vec![InquiryQuantitySelection {
        line_id: "line-b".into(),
        allocation_id: Some("address-b".into()),
        price_id: Some("price-b".into()),
        quantity: qty(quantity),
    }]
}

#[test]
fn unaccepted_new_feedback_preserves_old_accepted_unaffected_goods_and_fees() {
    let (mut inquiry, complete) = two_line_accepted_inquiry();
    inquiry
        .reinquire(
            1,
            "sales",
            InquiryReinquiryData {
                reason: "重询 A 商品与费用".into(),
                line_ids: vec!["line".into()],
                fee_ids: vec!["fee".into()],
                concerns: String::new(),
                requirement: None,
            },
            now(),
        )
        .unwrap();
    inquiry.publish(1, "buyer", complete, now()).unwrap();
    assert_eq!(inquiry.feedback_round, 2);
    inquiry.accepted_feedback(1, now()).unwrap();
    assert!(inquiry.accepted_feedback(2, now()).is_err());
    assert!(inquiry.reserve_quantities("affected-a", "order-a", 1, selected("1"), now()).is_err());
    inquiry.reserve_quantities("unaffected-b", "order-b", 1, selected_b("2"), now()).unwrap();
    assert!(inquiry.ensure_fee_available(1, "fee", now()).is_err());
    inquiry.ensure_fee_available(1, "fee-b", now()).unwrap();
    let sales = InquirySalesView::from_inquiry_at(&inquiry, now());
    assert!(sales.fee_statuses.iter().all(|status| status.feedback_round == 1));
    assert!(
        sales.fee_statuses.iter().find(|status| status.fee_id == "fee").unwrap().unavailable_reason.is_some()
    );
    assert!(
        sales
            .fee_statuses
            .iter()
            .find(|status| status.fee_id == "fee-b")
            .unwrap()
            .unavailable_reason
            .is_none()
    );
    let procurement = InquiryProcurementView::from_scoped_at(&inquiry, &["line-b".into()], now());
    assert_eq!(procurement.fee_statuses.len(), 1);
    assert_eq!(procurement.fee_statuses[0].feedback_round, 1);
    assert_eq!(procurement.fee_statuses[0].fee_id, "fee-b");
    inquiry.accept(1, "sales", 2, now()).unwrap();
    assert!(inquiry.accepted_feedback(1, now()).is_err());
    assert!(inquiry.reserve_quantities("old-b", "order-old", 1, selected_b("1"), now()).is_err());
    inquiry.reserve_quantities("new-a", "order-a", 2, selected("1"), now()).unwrap();
    inquiry.reserve_quantities("new-b", "order-new", 2, selected_b("1"), now()).unwrap();
}

#[test]
fn repeated_unaccepted_rounds_keep_the_union_of_affected_scopes_blocked() {
    let (mut inquiry, complete) = two_line_accepted_inquiry();
    inquiry
        .reinquire(
            1,
            "sales",
            InquiryReinquiryData {
                reason: "A 商品重询".into(),
                line_ids: vec!["line".into()],
                fee_ids: vec![],
                concerns: String::new(),
                requirement: None,
            },
            now(),
        )
        .unwrap();
    inquiry.publish(1, "buyer", complete.clone(), now()).unwrap();
    inquiry
        .reinquire(
            1,
            "sales",
            InquiryReinquiryData {
                reason: "追加 B 费用重询".into(),
                line_ids: vec![],
                fee_ids: vec!["fee-b".into()],
                concerns: String::new(),
                requirement: None,
            },
            now(),
        )
        .unwrap();
    inquiry.publish(1, "buyer", complete, now()).unwrap();
    assert!(inquiry.reserve_quantities("a", "order-a", 1, selected("1"), now()).is_err());
    inquiry.reserve_quantities("b", "order-b", 1, selected_b("1"), now()).unwrap();
    inquiry.ensure_fee_available(1, "fee", now()).unwrap();
    assert!(inquiry.ensure_fee_available(1, "fee-b", now()).is_err());
}

#[test]
fn requirement_revision_invalidates_every_old_accepted_reference() {
    let (mut inquiry, _) = two_line_accepted_inquiry();
    let mut changed = inquiry.draft.clone();
    changed.lines[0].quantity = qty("12");
    changed.lines[0].allocations[0].quantity = qty("12");
    inquiry
        .reinquire(
            1,
            "sales",
            InquiryReinquiryData {
                reason: "修改 A 需求量".into(),
                line_ids: vec!["line".into()],
                fee_ids: vec![],
                concerns: String::new(),
                requirement: Some(changed),
            },
            now(),
        )
        .unwrap();
    assert_eq!(inquiry.requirement_version, 2);
    assert!(inquiry.accepted_feedback(1, now()).is_err());
    assert!(inquiry.reserve_quantities("b", "order-b", 1, selected_b("1"), now()).is_err());
    assert!(inquiry.ensure_fee_available(1, "fee-b", now()).is_err());
}

#[test]
fn acceptance_rechecks_ledger_changed_after_feedback_publication() {
    let (mut inquiry, mut complete) = two_line_accepted_inquiry();
    inquiry
        .reinquire(
            1,
            "sales",
            InquiryReinquiryData {
                reason: "A 商品重询".into(),
                line_ids: vec!["line".into()],
                fee_ids: vec![],
                concerns: String::new(),
                requirement: None,
            },
            now(),
        )
        .unwrap();
    complete.lines[1].conclusion = InquirySupplyConclusion::PartiallyAvailable;
    complete.lines[1].available_quantity = qty("5");
    inquiry.publish(1, "buyer", complete, now()).unwrap();
    inquiry.reserve_quantities("b", "order-b", 1, selected_b("6"), now()).unwrap();
    let before = inquiry.clone();
    assert!(inquiry.accept(1, "sales", 2, now()).is_err());
    assert_eq!(inquiry, before);
    assert_eq!(inquiry.acceptances.last().unwrap().feedback_round, 1);
}

#[test]
fn transfer_progress_and_quote_validity_are_independent_of_processing_status() {
    use crate::dto::sales_inquiry::{InquiryQuoteValidity, InquirySalesProgress, InquiryTransferStatus};
    let mut inquiry = accepted();
    let progress = InquirySalesProgress::from_inquiry(&inquiry, now());
    assert_eq!(progress.transfer_status, InquiryTransferStatus::NotStarted);
    assert_eq!(progress.quote_validity, InquiryQuoteValidity::Valid);
    assert!(progress.can_create_sales);
    inquiry.reserve_quantities("first", "sales-1", 1, selected("4"), now()).unwrap();
    assert_eq!(
        InquirySalesProgress::from_inquiry(&inquiry, now()).transfer_status,
        InquiryTransferStatus::Partial
    );
    inquiry.consume_quantities("first").unwrap();
    inquiry.reserve_quantities("second", "sales-2", 1, selected("6"), now()).unwrap();
    let complete = InquirySalesProgress::from_inquiry(&inquiry, now());
    assert_eq!(complete.transfer_status, InquiryTransferStatus::Complete);
    assert!(!complete.can_create_sales);
    inquiry.release_quantities("second").unwrap();
    let expired = InquirySalesProgress::from_inquiry(&inquiry, later());
    assert_eq!(expired.transfer_status, InquiryTransferStatus::Partial);
    assert_eq!(expired.quote_validity, InquiryQuoteValidity::Expired);
    assert!(!expired.can_create_sales);
    inquiry.close(1, "sales", "关闭".into(), now()).unwrap();
    assert!(!InquirySalesProgress::from_inquiry(&inquiry, now()).can_create_sales);
}

#[test]
fn unoccurred_fee_can_decrease_but_occurred_fee_remains_protected() {
    let mut inquiry = accepted();
    inquiry.allocate_fee("allocation", "sales-order", 1, "fee", amount("100"), now()).unwrap();
    inquiry.reserve_fee(fee_reservation("expense", "100"), now()).unwrap();
    inquiry.adjust_fee("expense", amount("60"), now()).unwrap();
    assert_eq!(inquiry.fee_available_amount("fee", "allocation", 1, now()).unwrap(), amount("40"));
    inquiry.register_fee("expense").unwrap();
    assert!(inquiry.adjust_fee("expense", amount("50"), now()).is_err());
    assert!(inquiry.release_fee("expense").is_err());
}

#[test]
fn save_draft_updates_deadline_and_note_without_changing_submitted_history() {
    let mut inquiry = accepted();
    inquiry.status = InquiryStatus::AwaitingSalesSupplement;
    let snapshots = inquiry.requirements.clone();
    let mut data = inquiry.draft.clone();
    data.note = "更新后的交付说明".into();
    inquiry.save_draft(1, data.clone(), Some(now())).unwrap();
    assert_eq!(inquiry.deadline_at, now());
    assert_eq!(inquiry.draft, data);
    assert_eq!(inquiry.requirements, snapshots);
    inquiry.save_draft(1, data.clone(), None).unwrap();
    assert_eq!(inquiry.deadline_at, now());
    inquiry.save_draft(1, data, Some(Instant::from_unix_secs(0))).unwrap();
    assert_eq!(inquiry.deadline_at.unix_secs(), 0);
}

#[test]
fn save_draft_failure_preserves_deadline_and_note() {
    let initial = inquiry();
    let mut invalid = initial.draft.clone();
    invalid.note = "字".repeat(4001);
    for (version, data) in [(99, initial.draft.clone()), (1, invalid)] {
        let mut inquiry = initial.clone();
        assert!(inquiry.save_draft(version, data, Some(now())).is_err());
        assert_eq!(inquiry, initial);
    }
    let mut closed = initial;
    closed.status = InquiryStatus::Closed;
    let original = closed.clone();
    assert!(closed.save_draft(1, demand(), Some(now())).is_err());
    assert_eq!(closed, original);
}

#[test]
fn feedback_draft_saves_missing_quote_values_and_blocks_publish_until_completed() {
    let mut inquiry = inquiry();
    inquiry.submit(1, "sales", String::new(), now()).unwrap();
    let mut data = feedback("10", InquirySupplyConclusion::Available);
    data.lines[0].prices[0].unit_price_gross = None;
    data.lines[0].prices[0].tax_rate = None;
    inquiry.save_feedback_work(1, "buyer", vec!["line".into()], data.clone(), now()).unwrap();
    let encoded = serde_json::to_value(&inquiry).unwrap();
    let mut inquiry: SalesInquiry = serde_json::from_value(encoded).unwrap();
    assert_eq!(inquiry.feedback_work[0].data, data);
    let merged = inquiry.merge_feedback_work(&[("buyer".into(), vec!["line".into()])]).unwrap();
    let before = inquiry.clone();
    assert!(
        matches!(inquiry.publish(1, "buyer", merged, now()), Err(Error::ValidationError(message)) if message == "请填写含税供给单价后再发布")
    );
    assert_eq!(inquiry, before);

    data.lines[0].prices[0].unit_price_gross = Some("12.34".parse().unwrap());
    assert!(
        matches!(inquiry.publish(1, "buyer", data.clone(), now()), Err(Error::ValidationError(message)) if message == "请填写报价税率后再发布")
    );
    assert_eq!(inquiry, before);
    data.lines[0].prices[0].tax_rate = Some("0.13".parse().unwrap());
    inquiry.save_feedback_work(1, "buyer", vec!["line".into()], data, now()).unwrap();
    let merged = inquiry.merge_feedback_work(&[("buyer".into(), vec!["line".into()])]).unwrap();
    inquiry.publish(1, "buyer", merged, now()).unwrap();
    assert_eq!(inquiry.status, InquiryStatus::AwaitingSales);
    inquiry.accept(1, "sales", 1, now()).unwrap();
    inquiry.reserve_quantities("reserve", "order", 1, selected("10"), now()).unwrap();
}

#[test]
fn feedback_draft_keeps_explicit_zero_distinct_from_missing_and_checks_expiry() {
    let mut data = feedback("10", InquirySupplyConclusion::Available);
    let old_json = serde_json::to_value(&data).unwrap();
    assert_eq!(old_json["lines"][0]["prices"][0]["unit_price_gross"], "12.34");
    assert_eq!(serde_json::from_value::<InquiryFeedbackData>(old_json).unwrap(), data);
    data.lines[0].prices[0].unit_price_gross = Some("0".parse().unwrap());
    data.lines[0].prices[0].tax_rate = Some("0".parse().unwrap());
    data.validate(&demand(), now()).unwrap();
    let encoded = serde_json::to_value(&data).unwrap();
    assert_eq!(encoded["lines"][0]["prices"][0]["unit_price_gross"], "0");
    data.lines[0].prices[0].valid_until = now();
    assert!(
        matches!(data.validate(&demand(), now()), Err(Error::ValidationError(message)) if message == "供给报价已过期")
    );
}

#[test]
fn feedback_draft_missing_prices_do_not_bypass_scope_or_version_guards() {
    let mut inquiry = inquiry();
    inquiry.submit(1, "sales", String::new(), now()).unwrap();
    let mut data = feedback("10", InquirySupplyConclusion::Available);
    data.lines[0].prices[0].unit_price_gross = None;
    let before = inquiry.clone();
    assert!(inquiry.save_feedback_work(2, "buyer", vec!["line".into()], data.clone(), now()).is_err());
    assert_eq!(inquiry, before);
    assert!(matches!(
        inquiry.save_feedback_work(1, "buyer", vec!["outside".into()], data, now()),
        Err(Error::Forbidden(_))
    ));
    assert_eq!(inquiry, before);
}

#[test]
fn partial_feedback_uses_demand_addresses_and_shared_available_balance() {
    let mut inquiry = inquiry();
    let first = &mut inquiry.draft.lines[0].allocations[0];
    first.quantity = qty("6");
    let mut second = first.clone();
    second.allocation_id = "address-two".into();
    second.quantity = qty("4");
    inquiry.draft.lines[0].allocations.push(second);
    inquiry.submit(1, "sales", String::new(), now()).unwrap();
    inquiry.publish(1, "buyer", feedback("5", InquirySupplyConclusion::PartiallyAvailable), now()).unwrap();
    inquiry.accept(1, "sales", 1, now()).unwrap();
    let mut at_second = selected("5");
    at_second[0].allocation_id = Some("address-two".into());
    assert!(inquiry.reserve_quantities("address-over", "sale", 1, at_second.clone(), now()).is_err());
    inquiry.reserve_quantities("first", "sale", 1, selected("4"), now()).unwrap();
    at_second[0].quantity = qty("2");
    assert!(inquiry.reserve_quantities("total-over", "sale", 1, at_second.clone(), now()).is_err());
    at_second[0].quantity = qty("1");
    inquiry.reserve_quantities("second", "sale", 1, at_second, now()).unwrap();
    assert!(inquiry.reserve_quantities("exhausted", "sale", 1, selected("1"), now()).is_err());
}

#[test]
fn feedback_draft_rejects_multiple_prices_without_mutation() {
    let mut inquiry = inquiry();
    inquiry.submit(1, "sales", String::new(), now()).unwrap();
    let before = inquiry.clone();
    let mut data = feedback("10", InquirySupplyConclusion::Available);
    let extra = data.lines[0].prices[0].clone();
    data.lines[0].prices.push(extra);
    assert!(inquiry.save_feedback_work(1, "buyer", vec!["line".into()], data, now()).is_err());
    assert_eq!(inquiry, before);
}

#[test]
fn fee_scope_and_expiry_are_derived_and_cannot_be_overridden() {
    let mut inquiry = inquiry();
    inquiry.submit(1, "sales", String::new(), now()).unwrap();
    let mut data = feedback("10", InquirySupplyConclusion::Available);
    data.fees[0].line_ids = vec!["outside".into()];
    data.fees[0].valid_until = Instant::from_unix_secs(99999);
    inquiry.save_feedback_work(1, "buyer", vec!["line".into()], data.clone(), now()).unwrap();
    let fee = &inquiry.feedback_work.last().unwrap().data.fees[0];
    assert_eq!(fee.line_ids, vec!["line"]);
    assert_eq!(fee.valid_until, later());
    data.lines[0].prices[0].valid_until = Instant::from_unix_secs(0);
    inquiry.save_feedback_work(1, "buyer", vec!["line".into()], data, now()).unwrap();
    let saved = &inquiry.feedback_work.last().unwrap().data;
    assert_eq!(saved.fees[0].valid_until, Instant::from_unix_secs(0));
    assert!(saved.validate(&demand(), now()).is_err());
}

#[test]
fn feedback_work_rejects_reusing_another_buyers_fee_and_duplicate_ids() {
    let (mut inquiry, mut data) = two_line_accepted_inquiry();
    inquiry
        .reinquire(
            1,
            "sales",
            InquiryReinquiryData {
                reason: "重新报价".into(),
                line_ids: vec![],
                fee_ids: vec![],
                concerns: "费用".into(),
                requirement: None,
            },
            now(),
        )
        .unwrap();
    data.lines.retain(|line| line.line_id == "line-b");
    data.fees.retain(|fee| fee.fee_id == "fee");
    let before = inquiry.clone();
    assert!(inquiry.save_feedback_work(1, "buyer-b", vec!["line-b".into()], data.clone(), now()).is_err());
    assert_eq!(inquiry, before);
    data.fees[0].fee_id = "new".into();
    data.fees.push(data.fees[0].clone());
    assert!(inquiry.save_feedback_work(1, "buyer-b", vec!["line-b".into()], data, now()).is_err());
    assert_eq!(inquiry, before);
}

#[test]
fn whole_order_fee_uses_earliest_quote_after_combining_buyers() {
    let (_, mut data) = two_line_accepted_inquiry();
    data.fees[0].line_ids.clear();
    data.lines[1].prices[0].valid_until = Instant::from_unix_secs(200);
    data.refresh_fee_validity();
    assert_eq!(data.fees[0].valid_until, Instant::from_unix_secs(200));
    assert_eq!(data.fees[1].valid_until, Instant::from_unix_secs(200));
    data.fees[0].line_ids = vec!["line".into()];
    data.refresh_fee_validity();
    assert_eq!(data.fees[0].valid_until, later());
}

#[test]
fn completed_procurement_view_keeps_task_snapshot_and_excludes_later_rounds() {
    let mut inquiry = accepted();
    let mut later_feedback = inquiry.feedbacks[0].clone();
    later_feedback.round = 2;
    later_feedback.published_at = later();
    later_feedback.data.lines[0].public_note = "后续报价".into();
    inquiry.feedbacks.push(later_feedback);
    inquiry.draft.lines[0].product_name = "后续需求".into();
    let lines = vec!["line".into()];
    let view =
        InquiryProcurementView::from_completed_task(&inquiry, &lines, "buyer", 1, now(), now()).unwrap();
    assert_eq!(view.draft.lines[0].product_name, "测试商品");
    assert_eq!(view.feedbacks.len(), 1);
    assert_eq!(view.feedback_round, 1);
    assert_eq!(view.requirements.len(), 1);
    assert!(view.feedback_work.is_empty());
    assert!(view.fee_allocations.is_empty());
    assert!(view.fee_ledger.is_empty());
    assert!(view.fee_statuses.is_empty());
    let json = serde_json::to_value(view).unwrap();
    assert!(json["draft"]["lines"][0].get("sales_reference_price_gross").is_none());
    assert!(
        InquiryProcurementView::from_completed_task(&inquiry, &lines, "buyer", 2, now(), now()).is_none()
    );
    assert!(
        InquiryProcurementView::from_completed_task(&inquiry, &lines, "buyer", 1, later(), now()).is_none()
    );
    let empty = InquiryProcurementView::from_completed_task(&inquiry, &[], "buyer", 1, now(), now()).unwrap();
    assert!(empty.draft.lines.is_empty());
    assert!(empty.feedbacks[0].data.lines.is_empty());
    assert!(empty.feedbacks[0].data.fees.is_empty());
}

#[test]
fn completed_procurement_shared_fees_require_own_frozen_work() {
    let mut inquiry = accepted();
    let mut other_line = inquiry.requirements[0].data.lines[0].clone();
    other_line.line_id = "other".into();
    inquiry.requirements[0].data.lines.push(other_line);
    inquiry.feedbacks[0].data.fees[0].line_ids.clear();
    inquiry.feedback_work.push(InquiryFeedbackWork {
        requirement_version: 1,
        actor_id: "buyer".into(),
        line_ids: vec!["line".into()],
        data: inquiry.feedbacks[0].data.clone(),
        saved_at: now(),
        frozen: true,
    });
    let lines = vec!["line".into()];
    let view =
        InquiryProcurementView::from_completed_task(&inquiry, &lines, "buyer", 1, now(), now()).unwrap();
    assert_eq!(view.feedbacks[0].data.fees.len(), 1);
    assert_eq!(view.draft.lines.len(), 1);
    let other =
        InquiryProcurementView::from_completed_task(&inquiry, &lines, "another", 1, now(), now()).unwrap();
    assert!(other.feedbacks[0].data.fees.is_empty());
    inquiry.feedback_work[0].saved_at = later();
    let later_work =
        InquiryProcurementView::from_completed_task(&inquiry, &lines, "buyer", 1, now(), now()).unwrap();
    assert!(later_work.feedbacks[0].data.fees.is_empty());
}
