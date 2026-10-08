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
    let fields = ConfirmImport { version: 1, fields: extraction.draft(&doc).fields }.validate().unwrap().0;
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
fn draft_preserves_partial_values_and_nulls_conflicts_and_missing_fields() {
    let (mut doc, mut extraction) = sample();
    extraction.conflicts.push("payment_terms：同页存在两个付款约定".into());
    extraction.fields.remove(&TaxPoint);
    doc.pages[0].text.push_str("专票");
    extraction
        .fields
        .insert(InvoiceType, ExtractedField { value: "专票".into(), quote: "专票".into(), page: 1 });
    let draft = extraction.draft(&doc);
    assert_eq!(draft.fields.len(), 14);
    assert_eq!(draft.fields[&CustomerName].as_deref(), Some("客户有限公司"));
    assert_eq!(draft.fields[&InvoiceType].as_deref(), Some("增值税专用发票"));
    assert_eq!(draft.fields[&SignedAt].as_deref(), Some("2026-10-01"));
    assert!(draft.fields[&PaymentTerms].is_none());
    assert!(draft.fields[&TaxPoint].is_none());
    let json = serde_json::to_value(&draft).unwrap();
    assert!(json["fields"]["payment_terms"].is_null());
    assert_eq!(draft.warnings.len(), 1);
    assert!(extraction.fields.contains_key(&PaymentTerms));
}

#[test]
fn draft_allows_semantic_values_but_drops_unverifiable_evidence() {
    let (mut doc, mut extraction) = sample();
    doc.pages[0].text.push_str("支付50%定金，收货后支付尾款");
    extraction.fields.insert(
        PaymentTerms,
        ExtractedField {
            value: "先款 50%".into(), page: 1, quote: "支付50%定金，收货后支付尾款".into()
        },
    );
    extraction.fields.get_mut(&CustomerName).unwrap().quote = "不存在的原文".into();
    let draft = extraction.draft(&doc);
    assert_eq!(draft.fields[&PaymentTerms].as_deref(), Some("先款 50%"));
    assert!(draft.fields[&CustomerName].is_none());
    assert_eq!(draft.fields[&ContractNo].as_deref(), Some("HT-1"));
    extraction.fields.clear();
    assert!(extraction.draft(&doc).fields.values().all(Option::is_none));
}

#[test]
fn confirmation_accepts_edits_but_requires_valid_business_values() {
    let (doc, extraction) = sample();
    let mut command = ConfirmImport { version: 1, fields: extraction.draft(&doc).fields };
    command.fields.insert(ContractNo, Some("人工补充编号".into()));
    command.fields.insert(PaymentTerms, Some("先款 50%".into()));
    let (values, _) = command.validate().unwrap();
    assert_eq!(values.contract_no, "人工补充编号");
    assert_eq!(values.payment_code, "PREPAY_50");
    assert_eq!(extraction.fields[&ContractNo].value, "HT-1");
    for (field, value) in [
        (TaxPoint, None),
        (PaymentTerms, Some("收到发票后30天".into())),
        (SignedAt, Some("2026-02-30".into())),
        (ValidTo, Some("2020-01-01".into())),
    ] {
        let mut invalid = command.clone();
        invalid.fields.insert(field, value);
        assert!(invalid.validate().is_err());
    }
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
        stage: None,
        started_at: None,
        ocr: None,
        extraction: None,
        failure: None,
        result: None,
        confirmation: None,
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

#[test]
fn confirmation_rejects_stale_pending_and_changed_replays() {
    let mut task = task();
    let command = ConfirmImport { version: task.base.version, fields: BTreeMap::new() };
    assert!(task.check_confirmation(&command).is_err());
    task.status = ImportStatus::Review;
    assert!(task.check_confirmation(&command).unwrap());
    assert!(!task.begin(100).unwrap());
    let mut stale = command.clone();
    stale.version += 1;
    assert!(task.check_confirmation(&stale).is_err());
    task.status = ImportStatus::Succeeded;
    task.confirmation = Some(command.clone());
    assert!(!task.check_confirmation(&command).unwrap());
    let mut changed = command;
    changed.fields.insert(ContractNo, Some("other".into()));
    assert!(task.check_confirmation(&changed).is_err());
}

#[test]
fn successful_recognition_waits_for_confirmation_without_archiving() {
    let (doc, mut extraction) = sample();
    extraction.fields.remove(&PaymentTerms);
    extraction.conflicts.push("payment_terms：需要用户确认".into());
    let mut task = task();
    assert!(task.finish_recognition(doc.clone(), extraction.clone()).is_err());
    task.begin(100).unwrap();
    task.finish_recognition(doc, extraction).unwrap();
    assert_eq!(task.status, ImportStatus::Review);
    assert!(task.result.is_none());
    assert!(task.confirmation.is_none());
    let view = ImportView::from(task);
    assert!(view.failure.is_none());
    assert!(view.draft.unwrap().fields[&PaymentTerms].is_none());
}

#[test]
fn historical_tasks_without_confirmation_remain_readable() {
    let mut value = serde_json::to_value(task()).unwrap();
    value.as_object_mut().unwrap().remove("confirmation");
    let restored: ContractImport = serde_json::from_value(value).unwrap();
    assert!(restored.confirmation.is_none());
    assert_eq!(restored.status, ImportStatus::Ready);
}

#[test]
fn progress_only_advances_in_processing_and_retry_restarts_from_file() {
    let stages =
        [ImportStage::ReadingFile, ImportStage::Ocr, ImportStage::AiExtract, ImportStage::PreparingReview];
    for status in [
        ImportStatus::Ready,
        ImportStatus::Processing,
        ImportStatus::Failed,
        ImportStatus::Review,
        ImportStatus::Succeeded,
    ] {
        for (index, current) in stages.iter().enumerate() {
            for (next_index, next) in stages.iter().enumerate() {
                let mut task = task();
                task.status = status;
                task.stage = Some(*current);
                let expected = status == ImportStatus::Processing && next_index == index + 1;
                assert_eq!(task.advance(*next).is_ok(), expected);
                assert_eq!(task.stage, Some(if expected { *next } else { *current }));
            }
        }
    }
    let mut task = task();
    task.begin(100).unwrap();
    task.advance(ImportStage::Ocr).unwrap();
    task.advance(ImportStage::AiExtract).unwrap();
    task.status = ImportStatus::Failed;
    assert_eq!(ImportView::from(task.clone()).stage, Some(ImportStage::AiExtract));
    task.begin(200).unwrap();
    assert_eq!(task.stage, Some(ImportStage::ReadingFile));
    task.advance(ImportStage::Ocr).unwrap();
    task.begin(800).unwrap();
    assert_eq!(task.stage, Some(ImportStage::ReadingFile));
}

#[test]
fn legacy_tasks_have_unknown_progress_and_views_serialize_stage_names() {
    let mut task = task();
    let mut legacy = serde_json::to_value(&task).unwrap();
    legacy.as_object_mut().unwrap().remove("stage");
    let restored: ContractImport = serde_json::from_value(legacy).unwrap();
    assert_eq!(restored.stage, None);
    assert_eq!(ImportView::from(restored).stage, None);
    task.begin(100).unwrap();
    task.advance(ImportStage::Ocr).unwrap();
    let view = serde_json::to_value(ImportView::from(task)).unwrap();
    assert_eq!(view["stage"], "ocr");
    assert_eq!(view["status"], "processing");
}
