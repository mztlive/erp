use ContractField::*;

use super::*;

fn sample() -> (OcrDocument, ContractExtraction) {
    let data = [
        (ContractNo, "HT-1"),
        (CustomerName, "客户有限公司"),
        (CompanyName, "我方有限公司"),
        (SettlementName, "客户有限公司"),
        (PaymentTerms, "货到 30 天"),
        (InvoiceType, "增值税专用发票"),
        (TaxPoint, "13%"),
        (SignedAt, "2026年10月1日"),
        (ValidFrom, "2026-10-01"),
        (ValidTo, "2027-10-01"),
        (BusinessScope, "年节礼包"),
    ];
    let text = data.iter().map(|(_, v)| *v).collect::<Vec<_>>().join("\n");
    let doc = OcrDocument {
        provider: "test".into(),
        version: "1".into(),
        pages: vec![OcrPage { number: 1, text, blank: false, readable: true }],
    };
    let extraction = ContractExtraction {
        provider: "test".into(),
        version: "1".into(),
        conflicts: vec![],
        fields: data
            .into_iter()
            .map(|(key, value)| (key, ExtractedField { value: value.into(), page: 1, quote: value.into() }))
            .collect(),
    };
    (doc, extraction)
}
#[test]
fn validates_original_facts_without_defaults() {
    let (doc, extraction) = sample();
    doc.validate(1).unwrap();
    let fields = extraction.validate(&doc).unwrap();
    assert_eq!(fields.payment_code, "POSTPAY_NET30");
    assert_eq!(fields.tax_point, "13");
    assert_eq!(fields.signed_at.to_string(), "2026-10-01");
}
#[test]
fn rejects_missing_pages_unreadable_and_false_blank() {
    let (mut doc, _) = sample();
    assert_eq!(doc.validate(2).unwrap_err().code, "PAGE_COVERAGE");
    doc.pages[0].readable = false;
    assert_eq!(doc.validate(1).unwrap_err().page, Some(1));
    doc.pages[0].readable = true;
    doc.pages[0].blank = true;
    assert!(doc.validate(1).is_err());
}
#[test]
fn rejects_hallucination_conflicts_and_missing_fields() {
    let (doc, mut fields) = sample();
    fields.fields.get_mut(&CustomerName).unwrap().value = "不相关公司".into();
    assert_eq!(fields.validate(&doc).err().unwrap().code, "SOURCE_MISMATCH");
    let (_, mut fields) = sample();
    fields.conflicts.push("签章页不一致".into());
    assert_eq!(fields.validate(&doc).err().unwrap().code, "EXTRACTION_CONFLICT");
    fields.conflicts.clear();
    fields.fields.remove(&TaxPoint);
    assert_eq!(fields.validate(&doc).err().unwrap().field, Some(TaxPoint));
}
#[test]
fn refuses_semantically_different_terms_even_with_same_days() {
    let (mut doc, mut fields) = sample();
    doc.pages[0].text.push_str("\n验收后30天");
    fields.fields.insert(
        PaymentTerms,
        ExtractedField { value: "验收后30天".into(), quote: "验收后30天".into(), page: 1 },
    );
    assert_eq!(fields.validate(&doc).err().unwrap().code, "UNMATCHED_TERMS");
}
#[test]
fn identity_requires_unique_name_and_consistent_code() {
    let identity = MatchedIdentity {
        id: "p1".into(),
        version: 1,
        revision_id: Some("r1".into()),
        legal_name: "客户公司".into(),
        credit_code: Some("123".into()),
    };
    assert!(match_identity("客户公司", Some("123"), vec![identity.clone()]).is_ok());
    assert_eq!(
        match_identity("客户公司", Some("other"), vec![identity.clone()]).unwrap_err().code,
        "IDENTITY_CONFLICT"
    );
    assert!(match_identity("客户", None, vec![identity.clone()]).is_err());
    assert_eq!(
        match_identity("客户公司", None, vec![identity.clone(), identity]).unwrap_err().code,
        "MASTER_AMBIGUOUS"
    );
}
#[test]
fn http_command_rejects_manual_fields() {
    assert!(
        serde_json::from_value::<ImportCommand>(
            serde_json::json!({"request_key":"abcdefgh", "customer_name":"伪造客户"})
        )
        .is_err()
    );
}

fn task() -> ContractImport {
    ContractImport {
        base: entity_core::BaseModel::fake(),
        owner_id: "owner".into(),
        command: ImportCommand {
            request_key: "request-123".into(),
            expected_customer_id: None,
            revision_target: None,
        },
        source: ImportSource {
            file_asset_id: "file1".into(),
            file_name: "contract.pdf".into(),
            sha256: "digest".into(),
            page_count: 1,
        },
        status: ImportStatus::Ready,
        started_at: None,
        ocr: None,
        extraction: None,
        failure: None,
        result: None,
        customer_id: None,
    }
}

#[test]
fn task_replay_and_recovery_do_not_override_success() {
    let mut task = task();
    assert!(task.replay(&task.command, "digest").is_ok());
    assert!(task.replay(&task.command, "different").is_err());
    let mut changed = task.command.clone();
    changed.expected_customer_id = Some("another".into());
    assert!(task.replay(&changed, "digest").is_err());
    assert!(task.begin(100).unwrap());
    assert!(task.begin(699).is_err());
    assert!(task.begin(700).unwrap());
    task.status = ImportStatus::Succeeded;
    assert!(!task.begin(1000).unwrap());
    assert_eq!(task.status, ImportStatus::Succeeded);
}

#[test]
fn dates_tax_and_unknown_metadata_fail_closed() {
    let (mut doc, mut fields) = sample();
    for (key, text) in [(TaxPoint, "5%"), (SignedAt, "2026-02-30"), (ValidTo, "2020-01-01")] {
        let previous = fields.fields[&key].clone();
        doc.pages[0].text.push_str(text);
        fields.fields.insert(key, ExtractedField { value: text.into(), quote: text.into(), page: 1 });
        assert!(fields.validate(&doc).is_err());
        fields.fields.insert(key, previous);
    }
    doc.provider.clear();
    assert!(doc.validate(1).is_err());
}

#[test]
fn import_view_preserves_revision_target_and_server_recovery_deadline() {
    let mut task = task();
    task.command.expected_customer_id = Some("customer-a".into());
    task.command.revision_target = Some(RevisionTarget { contract_id: "contract-a".into(), version: 7 });
    task.begin(100).unwrap();
    let view = ImportView::from(task);
    assert_eq!(view.expected_customer_id.as_deref(), Some("customer-a"));
    assert_eq!(view.recoverable_at, Some(700));
    let target = view.revision_target.unwrap();
    assert_eq!(target.contract_id, "contract-a");
    assert_eq!(target.version, 7);
    let fresh = ImportView::from(self::task());
    assert!(fresh.revision_target.is_none());
    assert!(fresh.recoverable_at.is_none());
}
