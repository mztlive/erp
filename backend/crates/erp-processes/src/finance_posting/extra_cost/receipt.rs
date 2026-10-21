//! 财务附加费用命令以完整业务输入和真实凭证内容形成独立回执身份。
use application_core::{AuditActor, CommandReceipt};
use erp_core::ids::FileAssetId;
use erp_finance::dto::extra_cost::RegisterExtraCostRequest;
use erp_support::PendingAttachmentBatch;
use serde::Serialize;

use super::REGISTER_ACTION;
use crate::{Error, Result};

#[derive(Serialize)]
struct EvidenceContent {
    file_name: String,
    content_type: String,
    byte_size: u64,
    content_hmac: String,
}

pub(super) fn registration_receipt(
    actor: &AuditActor,
    sales_id: &str,
    request: &RegisterExtraCostRequest,
    pending: &dyn PendingAttachmentBatch,
) -> Result<CommandReceipt> {
    let mut input = request.clone();
    input.evidence_attachment_id = FileAssetId::new("same-evidence-content");
    input.idempotency_key = input.idempotency_key.trim().to_string();
    input.description = input.description.trim().to_string();
    let manifest = pending.content_manifest();
    if manifest.len() != 1 {
        return Err(Error::ValidationError("财务费用登记必须有一份可核对真实内容的凭证".into()));
    }
    let files = manifest
        .into_iter()
        .map(|file| EvidenceContent {
            file_name: file.file_name,
            content_type: file.content_type,
            byte_size: file.byte_size,
            content_hmac: file.content_hmac,
        })
        .collect::<Vec<_>>();
    Ok(CommandReceipt::from_payload(
        "sales-extra-cost-command-",
        actor.id(),
        REGISTER_ACTION,
        "cost_entry",
        &input.idempotency_key,
        &(sales_id, &input, files),
    )?)
}

#[cfg(test)]
mod tests {
    use erp_core::AccountKind;
    use erp_core::common::time::BusinessDate;
    use erp_core::ids::{SalesOrderLineId, SupplierAccountId};
    use erp_finance::dto::inquiry_fee::InquiryFeeReference;
    use erp_finance::entity::command_receipt::FinanceCommandReceipt;
    use erp_finance::entity::cost::ExtraCostCategory;
    use erp_support::{PendingFileAssetRequest, RegisterFileAssetRequest, RetentionClass, SensitivityClass};

    use super::*;
    use crate::attachments::PendingFileAssets;

    fn actor() -> AuditActor {
        AuditActor::new("actor".into(), "finance".into(), AccountKind::Admin)
    }
    fn request() -> RegisterExtraCostRequest {
        RegisterExtraCostRequest {
            category: ExtraCostCategory::Logistics,
            description: "运费".into(),
            gross_amount: "10.00".parse().unwrap(),
            input_tax_rate: "0.06".parse().unwrap(),
            occurred_on: BusinessDate::from_ymd(2026, 10, 21).unwrap(),
            supplier_id: Some(SupplierAccountId::new("supplier")),
            sales_order_line_id: Some(SalesOrderLineId::new("sales-line")),
            inquiry_source: Some(InquiryFeeReference {
                inquiry_id: "inquiry".into(),
                feedback_round: 1,
                fee_id: "fee".into(),
                allocation_id: "allocation".into(),
            }),
            evidence_attachment_id: FileAssetId::new("pending-file:evidence"),
            idempotency_key: "same-key".into(),
        }
    }
    fn files(reference: &str, key: &str, hmac: &str) -> PendingFileAssets {
        PendingFileAssets::prepare(
            vec![PendingFileAssetRequest {
                reference: reference.into(),
                registration: RegisterFileAssetRequest {
                    storage_object_key: key.into(),
                    file_name: "evidence.pdf".into(),
                    content_type: "application/pdf".into(),
                    byte_size: 10,
                    content_hmac: hmac.into(),
                    sensitivity_class: SensitivityClass::Sensitive,
                    retention_class: RetentionClass::LongTerm,
                    expires_at: None,
                },
            }],
            &actor(),
        )
        .unwrap()
    }
    #[test]
    fn full_payload_rejects_same_key_with_changed_business_input_or_evidence() {
        let pending = files("pending-file:evidence", "first-object", &"ab".repeat(32));
        let original = request();
        let command = registration_receipt(&actor(), "sales", &original, &pending).unwrap();
        let saved =
            FinanceCommandReceipt::resource(&command, "cost-entry".into(), "audit-event".into()).unwrap();
        let mut changes = Vec::new();
        let mut value = original.clone();
        value.input_tax_rate = "0.13".parse().unwrap();
        changes.push(value);
        let mut value = original.clone();
        value.category = ExtraCostCategory::Printing;
        changes.push(value);
        let mut value = original.clone();
        value.occurred_on = BusinessDate::from_ymd(2026, 10, 20).unwrap();
        changes.push(value);
        let mut value = original.clone();
        value.supplier_id = Some(SupplierAccountId::new("other-supplier"));
        changes.push(value);
        let mut value = original.clone();
        value.sales_order_line_id = Some(SalesOrderLineId::new("other-line"));
        changes.push(value);
        let mut value = original.clone();
        value.inquiry_source = None;
        changes.push(value);
        for changed in changes {
            let changed = registration_receipt(&actor(), "sales", &changed, &pending).unwrap();
            assert!(saved.resource_id(&changed).is_err());
        }
        let changed_file = files("pending-file:evidence", "second-object", &"cd".repeat(32));
        assert!(
            saved
                .resource_id(&registration_receipt(&actor(), "sales", &original, &changed_file).unwrap())
                .is_err()
        );
    }
    #[test]
    fn identical_evidence_content_replays_with_new_asset_id_key_and_temporary_reference() {
        let first = files("pending-file:first", "first-object", &"ab".repeat(32));
        let second = files("pending-file:second", "second-object", &"ab".repeat(32));
        let first_request = request();
        let mut second_request = request();
        second_request.evidence_attachment_id = FileAssetId::new("pending-file:second");
        let first = registration_receipt(&actor(), "sales", &first_request, &first).unwrap();
        let second = registration_receipt(&actor(), "sales", &second_request, &second).unwrap();
        let saved =
            FinanceCommandReceipt::resource(&first, "cost-entry".into(), "audit-event".into()).unwrap();
        assert_eq!(saved.resource_id(&second).unwrap(), "cost-entry");
    }
}
