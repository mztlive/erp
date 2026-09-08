use super::*;
use serde::de::DeserializeOwned;
use serde_json::json;

/// 由真实实体的持久化形状构造两次提交，避免以展示 DTO 自证版本正确。
fn entity<T: DeserializeOwned>(id: &str, fields: serde_json::Value) -> T {
    let mut value = serde_json::to_value(entity_core::BaseModel::new(id.to_string())).unwrap();
    value
        .as_object_mut()
        .unwrap()
        .extend(fields.as_object().unwrap().clone());
    serde_json::from_value(value).unwrap()
}

/// 为不同提交建立可区分的商品与金额。
fn target(title: &str, amount: &str) -> LineStateMap {
    HashMap::from([(
        "line-1".into(),
        DiffLineState {
            line_no: 1,
            title: title.into(),
            amount: amount.into(),
            quantity: Some("1 盒".into()),
            unit_price: None,
            due: None,
        },
    )])
}

/// 销售变更的旧金额、客户与明细按提交读取，当前草稿原因和类型不得混入。
#[test]
fn sales_change_history_keeps_frozen_customer_amount_and_lines() {
    let change: SalesChangeOrder = entity(
        "change",
        json!({
            "status": "DRAFT", "created_by": "starter", "updated_by": "starter",
            "sales_order_id": "sales", "base_revision_id": "base", "change_type": "AMOUNT",
            "reason": "当前草稿原因", "current_submission_id": "s2"
        }),
    );
    let mut context = SalesChangeBriefContext::default();
    for (version, amount, customer) in [(1, "100.00", "原客户"), (2, "200.00", "新客户")] {
        let id = format!("s{version}");
        let submission = entity(
            &id,
            json!({
                "status": "IN_REVIEW", "created_by": "starter", "updated_by": "starter",
                "sales_change_order_id": "change", "submission_no": version, "base_revision_id": "base",
                "sales_order_id": "sales", "working_copy_id": "draft", "working_copy_version": version,
                "business_type": "GOODS_SERVICE", "customer_id": "customer", "settlement_party_id": "party",
                "customer_snapshot": {"customer_name": customer},
                "payment_term_snapshot": {"payment_term_code": "NET30", "payment_term_name": "月结 30 天"},
                "invoice_requirement_snapshot": {"invoice_type": "普票", "tax_point": "0"},
                "gross_amount": amount, "net_amount": amount, "tax_amount": "0.00",
                "submitted_at": 1800000000, "submitted_by": "starter"
            }),
        );
        context.submissions.insert(id.clone(), submission);
        context.target_lines.insert(id, target(customer, amount));
    }
    let mut fact = WorkbenchObjectFact::from_authority(erp_workflow::ports::ObjectFact::new(
        "change",
        "销售变更",
        "starter",
    ));
    sales(&mut fact, &change, Some("XS1"), &context);
    assert_versions(&fact, "原客户", "新客户");
}

/// 采购变更的正式 CS 序号和提交 ID 均指向同一冻结内容。
#[test]
fn purchase_change_history_keeps_frozen_supplier_amount_and_lines() {
    let mut change: PurchaseChangeOrder = entity(
        "change",
        json!({
            "status": "DRAFT", "created_by": "starter", "updated_by": "starter",
            "purchase_order_id": "purchase", "base_revision_id": "base", "reason": "当前草稿原因",
            "current_submission_id": "s2", "approval_subject_version": 2
        }),
    );
    let mut context = PurchaseChangeBriefContext::default();
    for (version, amount, supplier) in [(1, "100.00", "原供应商"), (2, "200.00", "新供应商")] {
        let id = format!("s{version}");
        let submission = entity(
            &id,
            json!({
                "purchase_change_order_id": "change", "submission_no": format!("CS-{version:06}"),
                "base_revision_id": "base", "supplier_id": "supplier", "purchase_type": "PHYSICAL",
                "fulfillment_responsibility": "WAREHOUSE", "supplier_revision_id": "supplier-revision",
                "supplier_snapshot": {"supplier_name": supplier},
                "payment_term_snapshot": {"payment_term_code": "NET30", "prepay_gate": false},
                "gross_amount": amount, "net_amount": amount, "tax_amount": "0.00", "status": "PENDING"
            }),
        );
        context.submissions.insert(id.clone(), submission);
        context.target_lines.insert(id, target(supplier, amount));
    }
    let mut fact = WorkbenchObjectFact::from_authority(erp_workflow::ports::ObjectFact::new(
        "change",
        "采购变更",
        "starter",
    ));
    purchase(&mut fact, &change, Some("PO1"), &context);
    assert_versions(&fact, "原供应商", "新供应商");

    // 迁移后 CS 编号与审批计数不同，仅当前明确指向的提交可绑定审批版本。
    change.approval_subject_version = 5;
    fact.display.subject_briefs.clear();
    purchase(&mut fact, &change, Some("PO1"), &context);
    assert!(!fact.display.subject_briefs.contains_key("1"));
    assert!(!fact.display.subject_briefs.contains_key("2"));
    assert_eq!(
        fact.display.subject_briefs["5"].counterparty_label.as_deref(),
        Some("新供应商")
    );
    assert!(fact.display.subject_briefs.contains_key("s1"));
}

/// 两种变更必须提供相同的版本隔离与缺失规则。
fn assert_versions(fact: &WorkbenchObjectFact, old_party: &str, new_party: &str) {
    for (version, party, amount) in [("1", old_party, "100"), ("2", new_party, "200")] {
        let display = &fact.display.subject_briefs[version];
        assert_eq!(display.counterparty_label.as_deref(), Some(party));
        let source = display.brief_source.as_ref().unwrap();
        assert!(source.amount_label.as_ref().unwrap().contains(amount));
        assert!(source.lines[0].title.contains(party));
        assert!(!source
            .extra_sections
            .iter()
            .any(|section| section.label == "原因" || section.label == "变更类型"));
        assert!(!source.list_summary.contains("当前草稿原因"));
        assert_eq!(
            source.amount_label,
            fact.display.subject_briefs[&format!("s{version}")]
                .brief_source
                .as_ref()
                .unwrap()
                .amount_label
        );
    }
    assert!(crate::workbench::approval_list::document_summary(fact, Some(3)).is_none());
}
