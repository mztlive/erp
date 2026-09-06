//! Entity BSON round-trip contracts moved out of import entity modules.

use crate::entity::legacy_import::{
    LegacyImportBatch, LegacyImportBatchData, LegacyImportBatchStatus, LegacyImportConfirmation,
    LegacyImportConfirmationData, LegacyImportRow, LegacyImportRowData, ParseStatus,
};
use erp_core::common::time::BusinessDate;
use erp_core::ids::{
    LegacyImportBatchId, LegacyImportConfirmationId, LegacyImportRowId, SourceSystemId, WorkItemId,
};

fn batch_data() -> LegacyImportBatchData {
    LegacyImportBatchData {
        batch_no: "IMP-10".to_string(),
        source_system_id: SourceSystemId::new("source-1"),
        source_object_set: "CUSTOMER".to_string(),
        baseline_date: BusinessDate::from_ymd(2026, 8, 14).unwrap(),
        import_rule_version: "rule-1".to_string(),
        source_file_hmac: None,
        status: LegacyImportBatchStatus::PendingValidation,
        total_rows: 1,
        success_rows: 0,
        failed_rows: 0,
        failure_code_summary: None,
        confirmation_status_summary: None,
    }
}

fn confirmation_data() -> LegacyImportConfirmationData {
    LegacyImportConfirmationData {
        batch_id: LegacyImportBatchId::new("batch-1"),
        confirmation_scope: "SALES".to_string(),
        owner_role: "role-sales".to_string(),
        batch_version: 1,
        trial_version: 1,
        import_rule_version: "rule-1".to_string(),
        work_item_id: WorkItemId::new("work-item-1"),
    }
}

fn row_data() -> LegacyImportRowData {
    LegacyImportRowData {
        batch_id: LegacyImportBatchId::new("batch-1"),
        source_object_type: "CUSTOMER".to_string(),
        source_row_key: "key-1".to_string(),
        normalized_payload_reference: "payload:row-11".to_string(),
    }
}

#[test]
fn bson_roundtrip_preserves_batch() {
    let batch = LegacyImportBatch::new(LegacyImportBatchId::new("b-10"), batch_data()).unwrap();
    let roundtrip: LegacyImportBatch =
        mongodb::bson::deserialize_from_document(mongodb::bson::serialize_to_document(&batch).unwrap())
            .unwrap();
    assert_eq!(roundtrip, batch);
}

#[test]
fn bson_roundtrip_preserves_confirmation() {
    let confirmation =
        LegacyImportConfirmation::new(LegacyImportConfirmationId::new("c-10"), confirmation_data()).unwrap();
    let roundtrip: LegacyImportConfirmation = mongodb::bson::deserialize_from_document(
        mongodb::bson::serialize_to_document(&confirmation).unwrap(),
    )
    .unwrap();
    assert_eq!(roundtrip, confirmation);
}

#[test]
fn bson_roundtrip_preserves_row() {
    let mut row = LegacyImportRow::new(LegacyImportRowId::new("row-11"), row_data()).unwrap();
    row.mark_parse_result(ParseStatus::Valid, None, None).unwrap();
    let roundtrip: LegacyImportRow =
        mongodb::bson::deserialize_from_document(mongodb::bson::serialize_to_document(&row).unwrap())
            .unwrap();
    assert_eq!(roundtrip, row);
}
