//! 生成或恢复演示供应商。没有公司主体时跳过，不改其他资料。

use std::str::FromStr;

use application_core::AuditActor;
use erp_core::common::time::BusinessDate;
use erp_core::ids::PartyId;
use erp_core::money::Rate;
use erp_party::PartyExt;
use erp_party::repository::PartyRepositoryExt;
use erp_supplier::repository::SupplierProfileCommandRepositoryExt;
use erp_supplier::{
    CapabilityCode, HandoverSupplierRequest, InvoiceType, QualificationType, ReconciliationCycle,
    SaveSupplierProfileRequest, SettlementMode, SupplierExt, SupplierProfileAddressInput,
    SupplierProfileBankAccountInput, SupplierProfileCapabilityOwnerInput, SupplierProfileContactInput,
    SupplierProfileQualificationInput, SupplierProfileRatingInput, SupplierRating,
};
use persistence_core::NoTransaction;

use super::ensure_dictionary::EnsureOutcome;
use super::plan::{self, DemoStep};
use super::record::{self, DemoMasterRecord};
use super::{DemoMasterDataService, lifecycle, spec};
use crate::{Error, Result};

impl DemoMasterDataService {
    pub(super) async fn ensure_supplier(
        &self,
        actor: &AuditActor,
        step: &DemoStep,
        company_party_id: Option<&str>,
        notices: &mut Vec<String>,
    ) -> Result<EnsureOutcome> {
        let Some(company_party_id) = company_party_id else {
            return Ok(EnsureOutcome::Notice("还没有公司主体，未生成供应商".to_string()));
        };
        if let Some(supplier_id) = self.supplier_command(&step.key).await? {
            return self.adopt_supplier(actor, step, &supplier_id, notices).await;
        }
        let Some(maintainer) = self.role_actor(&spec::foundation_spec().supplier_maintainer_account).await?
        else {
            return Ok(EnsureOutcome::Notice("没有可用的采购账号，未生成供应商".to_string()));
        };
        let view = match self
            .suppliers()
            .create(supplier_request(step, maintainer.id(), company_party_id)?, actor)
            .await
        {
            Ok(view) => view,
            Err(Error::ValidationError(message) | Error::Forbidden(message)) => {
                return Ok(EnsureOutcome::Notice(format!("未生成供应商：{message}")));
            },
            Err(error) => return Err(error),
        };
        self.remember_created_supplier(actor, step, &view.supplier_id).await
    }

    async fn remember_created_supplier(
        &self,
        actor: &AuditActor,
        step: &DemoStep,
        supplier_id: &str,
    ) -> Result<EnsureOutcome> {
        let party = self
            .db
            .parties()
            .find_by_party_no_including_deleted(&plan::supplier_party_no(step.ordinal), &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("演示供应商创建后未能读回主体".to_string()))?;
        let supplier = self
            .db
            .supplier_accounts()
            .find_by_id_including_deleted(supplier_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("演示供应商创建后未能读回".to_string()))?;
        let created = supplier.base.deleted_at == entity_core::NOT_DELETED_TIMESTAMP
            && party.base.deleted_at == entity_core::NOT_DELETED_TIMESTAMP;
        if party.base.deleted_at != entity_core::NOT_DELETED_TIMESTAMP {
            lifecycle::restore_party(&self.db, actor, &party.base.id).await?;
        }
        if supplier.base.deleted_at != entity_core::NOT_DELETED_TIMESTAMP {
            lifecycle::restore_supplier(&self.db, actor, supplier_id).await?;
        }
        record::save(&self.db, &supplier_record(step, supplier_id, &party.base.id)).await?;
        Ok(if created { EnsureOutcome::Created } else { EnsureOutcome::Restored })
    }

    async fn adopt_supplier(
        &self,
        actor: &AuditActor,
        step: &DemoStep,
        supplier_id: &str,
        notices: &mut Vec<String>,
    ) -> Result<EnsureOutcome> {
        let outcome = self.remember_created_supplier(actor, step, supplier_id).await?;
        self.align_supplier_maintainer(actor, step, supplier_id, notices).await?;
        Ok(outcome)
    }

    async fn align_supplier_maintainer(
        &self,
        actor: &AuditActor,
        step: &DemoStep,
        supplier_id: &str,
        notices: &mut Vec<String>,
    ) -> Result<()> {
        let Some(maintainer) = self.role_actor(&spec::foundation_spec().supplier_maintainer_account).await?
        else {
            push_supplier_notice(notices, "没有可用的采购账号，演示供应商仍由原维护人负责");
            return Ok(());
        };
        let Some(supplier) =
            self.db.supplier_accounts().find_by_id_including_deleted(supplier_id, &mut NoTransaction).await?
        else {
            return Ok(());
        };
        if supplier.base.deleted_at != entity_core::NOT_DELETED_TIMESTAMP
            || supplier.maintainer_user_id == maintainer.id()
        {
            return Ok(());
        }
        let request = HandoverSupplierRequest {
            expected_version: supplier.base.version,
            target_user_id: maintainer.id().to_string(),
            target_org_unit_id: None,
            reason: "演示供应商交给采购账号".to_string(),
            idempotency_key: format!("demo-handover-{}", step.key),
        };
        if let Err(error) = self.suppliers().handover_supplier(supplier_id, request, actor).await {
            push_supplier_notice(notices, &format!("演示供应商未能交给采购账号：{error}"));
        }
        Ok(())
    }

    async fn supplier_command(&self, key: &str) -> Result<Option<String>> {
        let command =
            self.db.supplier_profile_commands().find_by_idempotency_key(key, &mut NoTransaction).await?;
        Ok(command.map(|command| command.supplier_id))
    }
}

fn supplier_request(
    step: &DemoStep,
    maintainer_id: &str,
    company_party_id: &str,
) -> Result<SaveSupplierProfileRequest> {
    let ordinal = step.ordinal;
    let date = super::demo_date()?;
    let rate = Rate::from_str("0.130000").map_err(|error| Error::Internal(error.to_string()))?;
    Ok(SaveSupplierProfileRequest {
        idempotency_key: step.key.clone(),
        party_no: Some(plan::supplier_party_no(ordinal)),
        supplier_no: Some(plan::supplier_no(ordinal)),
        expected_party_version: None,
        expected_supplier_version: None,
        legal_name: format!("演示供应商{ordinal:02}有限公司"),
        short_name: Some(format!("演示供应商{ordinal:02}")),
        unified_credit_code: None,
        contact: Some(SupplierProfileContactInput::new(
            format!("演示联系人{ordinal:02}"),
            format!("139{ordinal:08}"),
        )),
        clear_contact: false,
        address: Some(SupplierProfileAddressInput {
            address: format!("演示供应商路{ordinal:02}号"),
            contact_name: Some(format!("演示联系人{ordinal:02}")),
        }),
        clear_address: false,
        tax_no: None,
        clear_tax_profile: false,
        bank_account: Some(SupplierProfileBankAccountInput {
            bank_name: "演示银行".to_string(),
            account_number: format!("622202000000{ordinal:04}"),
        }),
        clear_bank_account: false,
        settlement_mode: SettlementMode::Prepayment,
        reconciliation_cycle: ReconciliationCycle::None,
        payment_term_snapshot: "PREPAY_50".to_string(),
        business_category: Some("演示礼品".to_string()),
        invoice_type: InvoiceType::VatSpecial,
        invoice_tax_rate: Some(rate),
        invoice_tax_rates: Some(vec![rate]),
        signing_entity_party_id: PartyId::new(company_party_id),
        payment_entity_party_id: PartyId::new(company_party_id),
        maintainer_user_id: Some(maintainer_id.to_string()),
        capability_owners: vec![capability_owner(maintainer_id)],
        capability_codes: vec![CapabilityCode::Physical],
        qualifications: vec![supplier_qualification(ordinal, date)?],
        rating: Some(supplier_rating(date)),
        effective_from: date,
        change_reason: "演示主数据".to_string(),
    })
}

fn capability_owner(maintainer_id: &str) -> SupplierProfileCapabilityOwnerInput {
    SupplierProfileCapabilityOwnerInput {
        capability_code: CapabilityCode::Physical,
        owner_user_id: maintainer_id.to_string(),
    }
}

fn supplier_qualification(ordinal: u16, date: BusinessDate) -> Result<SupplierProfileQualificationInput> {
    Ok(SupplierProfileQualificationInput {
        qualification_type: QualificationType::Contract,
        certificate_no: format!("DEMO-HT-{ordinal:02}"),
        issuer: None,
        valid_from: Some(date),
        valid_to: Some(super::demo_date_end()?),
        attachment_id: None,
        capability_codes: vec![CapabilityCode::Physical],
    })
}

fn supplier_rating(date: BusinessDate) -> SupplierProfileRatingInput {
    SupplierProfileRatingInput {
        initial_score: Some(90),
        rating: SupplierRating::A,
        current_score: 90,
        valid_from: date,
    }
}

fn supplier_record(step: &DemoStep, supplier_id: &str, party_id: &str) -> DemoMasterRecord {
    DemoMasterRecord {
        key: step.key.clone(),
        kind: step.kind.as_str().to_string(),
        entity_id: supplier_id.to_string(),
        related_ids: vec![party_id.to_string()],
        label: format!("演示供应商{:02}有限公司", step.ordinal),
        removed: false,
    }
}

fn push_supplier_notice(notices: &mut Vec<String>, text: &str) {
    if !notices.iter().any(|item| item == text) {
        notices.push(text.to_string());
    }
}
