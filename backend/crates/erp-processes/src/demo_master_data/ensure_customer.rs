//! 生成或恢复演示客户。

use application_core::AuditActor;
use erp_core::common::time::BusinessDate;
use erp_customer::repository::customer::{
    CustomerAssignmentRepositoryExt, CustomerProfileCommandRepositoryExt,
};
use erp_customer::{
    AddressType, AssignmentAction, AssignmentRole, CustomerAssignmentRequest, CustomerExt,
    CustomerProfileAddressInput, CustomerProfileContactInput, SaveCustomerProfileRequest,
};
use persistence_core::NoTransaction;

use super::ensure_dictionary::{EnsureOutcome, step_label};
use super::plan::DemoStep;
use super::record::{self, DemoMasterRecord};
use super::{DemoMasterDataService, lifecycle, spec};
use crate::adapters::scoped_customer_assignment_service;
use crate::{Error, Result};

impl DemoMasterDataService {
    pub(super) async fn ensure_customer(
        &self,
        actor: &AuditActor,
        step: &DemoStep,
        notices: &mut Vec<String>,
    ) -> Result<EnsureOutcome> {
        if let Some(existing) = self.customer_command(&step.key).await? {
            return self.adopt_customer(actor, step, &existing, notices).await;
        }
        let Some(owner) = self.role_actor(&spec::foundation_spec().customer_owner_account).await? else {
            return Ok(EnsureOutcome::Notice("没有可用的销售账号，未生成客户".to_string()));
        };
        let view = match self.customers().create(customer_request(step)?, &owner).await {
            Ok(view) => view,
            Err(Error::ValidationError(message) | Error::Forbidden(message)) => {
                return Ok(EnsureOutcome::Notice(format!("未生成客户：{message}")));
            },
            Err(error) => return Err(error),
        };
        let account = self
            .db
            .customer_accounts()
            .find_by_id_including_deleted(&view.customer_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("演示客户创建后未能读回".to_string()))?;
        let created = account.base.deleted_at == entity_core::NOT_DELETED_TIMESTAMP;
        if !created {
            lifecycle::restore_party(&self.db, actor, &view.party_id).await?;
            lifecycle::restore_customer(&self.db, actor, &view.customer_id).await?;
        }
        record::save(&self.db, &customer_record(step, &view.customer_id, &view.party_id)).await?;
        Ok(if created { EnsureOutcome::Created } else { EnsureOutcome::Restored })
    }

    async fn adopt_customer(
        &self,
        actor: &AuditActor,
        step: &DemoStep,
        customer_id: &str,
        notices: &mut Vec<String>,
    ) -> Result<EnsureOutcome> {
        let account = self
            .db
            .customer_accounts()
            .find_by_id_including_deleted(customer_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("演示客户命令缺少客户".to_string()))?;
        let party_id = account.party_id.to_string();
        let live = account.base.deleted_at == entity_core::NOT_DELETED_TIMESTAMP;
        if !live {
            restore_optional(lifecycle::restore_party(&self.db, actor, &party_id).await)?;
            lifecycle::restore_customer(&self.db, actor, customer_id).await?;
        }
        self.align_customer_owner(actor, customer_id, notices).await?;
        let face = super::names::customer(step.ordinal);
        self.rename_party_if_placeholder(actor, &party_id, face.legal_name, face.short_name).await?;
        record::save(&self.db, &customer_record(step, customer_id, &party_id)).await?;
        Ok(if live { EnsureOutcome::Skipped } else { EnsureOutcome::Restored })
    }

    async fn customer_command(&self, key: &str) -> Result<Option<String>> {
        let command =
            self.db.customer_profile_commands().find_by_idempotency_key(key, &mut NoTransaction).await?;
        Ok(command.map(|command| command.customer_id))
    }

    async fn align_customer_owner(
        &self,
        actor: &AuditActor,
        customer_id: &str,
        notices: &mut Vec<String>,
    ) -> Result<()> {
        let Some(owner) = self.role_actor(&spec::foundation_spec().customer_owner_account).await? else {
            push_unique(notices, "没有可用的销售账号，演示客户仍由原负责人维护");
            return Ok(());
        };
        let owners = self
            .db
            .customer_assignments()
            .current_owners(Some(&[customer_id.to_string()]), None, BusinessDate::today(), &mut NoTransaction)
            .await?;
        if owners.iter().any(|row| row.user_id == owner.id()) {
            return Ok(());
        }
        let request = CustomerAssignmentRequest {
            action: AssignmentAction::Assign,
            user_id: Some(owner.id().to_string()),
            assignment_role: Some(AssignmentRole::Owner),
            valid_from: Some(BusinessDate::today()),
            valid_to: None,
            assignment_id: None,
            change_reason: "演示客户交给销售账号".to_string(),
            version: None,
        };
        if let Err(error) = scoped_customer_assignment_service(self.db.clone(), self.rbac.clone())
            .apply_assignment(customer_id, request, actor)
            .await
        {
            push_unique(notices, &format!("演示客户未能交给销售账号：{error}"));
        }
        Ok(())
    }
}

fn customer_request(step: &DemoStep) -> Result<SaveCustomerProfileRequest> {
    let ordinal = step.ordinal;
    Ok(SaveCustomerProfileRequest {
        idempotency_key: step.key.clone(),
        expected_party_version: None,
        expected_customer_version: None,
        legal_name: super::names::customer(ordinal).legal_name.to_string(),
        short_name: Some(super::names::customer(ordinal).short_name.to_string()),
        unified_credit_code: None,
        default_payment_term_id: None,
        status: None,
        owner_user_id: None,
        contacts: Some(vec![
            CustomerProfileContactInput::new(super::names::customer(ordinal).contact.to_string())
                .with_mobile(super::names::customer(ordinal).phone.to_string())
                .with_is_default(true),
        ]),
        addresses: Some(vec![CustomerProfileAddressInput {
            existing_id: None,
            address_type: AddressType::Operating,
            contact_name: Some(super::names::customer(ordinal).contact.to_string()),
            address: Some(super::names::customer(ordinal).address.to_string()),
            is_default: true,
        }]),
        bank_accounts: None,
        effective_from: super::demo_date()?,
        change_reason: "演示主数据".to_string(),
    })
}

fn customer_record(step: &DemoStep, customer_id: &str, party_id: &str) -> DemoMasterRecord {
    DemoMasterRecord {
        key: step.key.clone(),
        kind: step.kind.as_str().to_string(),
        entity_id: customer_id.to_string(),
        related_ids: vec![party_id.to_string()],
        label: step_label(step),
        removed: false,
    }
}

fn restore_optional(result: Result<()>) -> Result<()> {
    match result {
        Ok(()) | Err(Error::NotFound(_)) => Ok(()),
        Err(error) => Err(error),
    }
}

fn push_unique(notices: &mut Vec<String>, text: &str) {
    if !notices.iter().any(|item| item == text) {
        notices.push(text.to_string());
    }
}
