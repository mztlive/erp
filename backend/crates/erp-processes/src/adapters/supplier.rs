//! Supplier party-fact, sensitive-token and file-asset adapters.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use erp_core::ids::{FileAssetId, PartyId};
use erp_identity::AccessControlExt;
use erp_identity::repository::prelude::*;
use erp_party::repository::prelude::*;
use erp_party::{PartyExt, SensitiveDataCodec, SensitiveFieldKind};
use erp_supplier::{
    AccountFactPort, AddressTypeFact, EffectiveRecordStatusFact, FileAssetFact, FileAssetFactsPort,
    PartyAddressFact, PartyBankAccountFact, PartyContactFact, PartyFactsPort, PartyListFact,
    PartyRevisionFact, PartyStatusFact, PartyTaxProfileFact, SensitiveFieldKindFact, SensitiveTokenPort,
};
use erp_support::FileAssetExt;
use mongodb::Database;
use persistence_core::{Executor, NoTransaction};

/// MongoDB adapter that reads party facts for supplier queries.
#[derive(Clone)]
pub struct MongoSupplierPartyFacts {
    db: Database,
}

impl MongoSupplierPartyFacts {
    /// Bind the adapter to `db`.
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// Wrap the adapter as a shared port.
    pub fn shared(db: Database) -> Arc<dyn PartyFactsPort> {
        Arc::new(Self::new(db))
    }
}

#[async_trait]
impl PartyFactsPort for MongoSupplierPartyFacts {
    async fn matching_current_party_ids_by_name(
        &self,
        keyword: &str,
        executor: &mut dyn Executor,
    ) -> erp_supplier::Result<Vec<PartyId>> {
        self.db
            .party()
            .matching_current_party_ids_by_name(keyword, executor)
            .await
            .map_err(erp_supplier::Error::from)
    }

    async fn list_with_current_revisions(
        &self,
        party_ids: &[PartyId],
        executor: &mut dyn Executor,
    ) -> erp_supplier::Result<(Vec<PartyListFact>, Vec<PartyRevisionFact>)> {
        let (parties, revisions) = self
            .db
            .party()
            .list_with_current_revisions(party_ids, executor)
            .await
            .map_err(erp_supplier::Error::from)?;
        Ok((
            parties.into_iter().map(party_list_fact).collect(),
            revisions.into_iter().map(party_revision_fact).collect(),
        ))
    }

    async fn find_with_current_revision(
        &self,
        party_id: &PartyId,
        executor: &mut dyn Executor,
    ) -> erp_supplier::Result<Option<(PartyListFact, Option<PartyRevisionFact>)>> {
        Ok(self
            .db
            .party()
            .find_with_current_revision(party_id, executor)
            .await
            .map_err(erp_supplier::Error::from)?
            .map(|(party, revision)| (party_list_fact(party), revision.map(party_revision_fact))))
    }

    async fn list_contacts(
        &self,
        party_id: &PartyId,
        executor: &mut dyn Executor,
    ) -> erp_supplier::Result<Vec<PartyContactFact>> {
        Ok(self
            .db
            .party_contacts()
            .list_by_party(party_id, executor)
            .await
            .map_err(erp_supplier::Error::from)?
            .into_iter()
            .map(party_contact_fact)
            .collect())
    }

    async fn list_addresses(
        &self,
        party_id: &PartyId,
        executor: &mut dyn Executor,
    ) -> erp_supplier::Result<Vec<PartyAddressFact>> {
        Ok(self
            .db
            .party_addresses()
            .list_by_party(party_id, executor)
            .await
            .map_err(erp_supplier::Error::from)?
            .into_iter()
            .map(party_address_fact)
            .collect())
    }

    async fn list_tax_profiles(
        &self,
        party_id: &PartyId,
        executor: &mut dyn Executor,
    ) -> erp_supplier::Result<Vec<PartyTaxProfileFact>> {
        Ok(self
            .db
            .party_tax_profiles()
            .list_by_party(party_id, executor)
            .await
            .map_err(erp_supplier::Error::from)?
            .into_iter()
            .map(party_tax_profile_fact)
            .collect())
    }

    async fn list_bank_accounts(
        &self,
        party_id: &PartyId,
        executor: &mut dyn Executor,
    ) -> erp_supplier::Result<Vec<PartyBankAccountFact>> {
        Ok(self
            .db
            .party_bank_accounts()
            .list_by_party(party_id, executor)
            .await
            .map_err(erp_supplier::Error::from)?
            .into_iter()
            .map(party_bank_account_fact)
            .collect())
    }

    async fn current_legal_names_by_party_ids(
        &self,
        party_ids: &[PartyId],
        executor: &mut dyn Executor,
    ) -> erp_supplier::Result<HashMap<String, String>> {
        let (parties, revisions) = self.list_with_current_revisions(party_ids, executor).await?;
        let revisions: HashMap<String, PartyRevisionFact> =
            revisions.into_iter().map(|revision| (revision.id.clone(), revision)).collect();
        Ok(parties
            .into_iter()
            .filter_map(|party| {
                party
                    .current_revision_id
                    .as_ref()
                    .and_then(|id| revisions.get(id))
                    .map(|revision| (party.id, revision.legal_name.clone()))
            })
            .collect())
    }
}

/// Adapter that issues party-owned sensitive reveal tokens for supplier facts.
#[derive(Clone)]
pub struct MongoSupplierSensitiveTokens {
    codec: Arc<SensitiveDataCodec>,
}

impl MongoSupplierSensitiveTokens {
    /// Wrap a codec as the supplier token port.
    pub fn shared(codec: Arc<SensitiveDataCodec>) -> Arc<dyn SensitiveTokenPort> {
        Arc::new(Self { codec })
    }
}

impl SensitiveTokenPort for MongoSupplierSensitiveTokens {
    fn issue_reveal_token(
        &self,
        kind: SensitiveFieldKindFact,
        record_id: &str,
        supplier_id: &str,
        expires_at: u64,
    ) -> erp_supplier::Result<String> {
        self.codec
            .issue_reveal_token(map_sensitive_kind(kind), record_id, supplier_id, expires_at)
            .map_err(map_party_to_supplier)
    }
}

/// MongoDB adapter that reads qualification attachment facts from support.
#[derive(Clone)]
pub struct MongoSupplierFileAssets {
    db: Database,
}

impl MongoSupplierFileAssets {
    /// Bind the adapter to `db`.
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// Wrap the adapter as a shared port.
    pub fn shared(db: Database) -> Arc<dyn FileAssetFactsPort> {
        Arc::new(Self::new(db))
    }
}

#[async_trait]
impl FileAssetFactsPort for MongoSupplierFileAssets {
    async fn find_by_id(
        &self,
        attachment_id: &FileAssetId,
        executor: &mut dyn Executor,
    ) -> erp_supplier::Result<Option<FileAssetFact>> {
        Ok(self
            .db
            .file_assets()
            .find_by_id(attachment_id.as_ref(), executor)
            .await
            .map_err(erp_supplier::Error::from)?
            .map(|asset| FileAssetFact {
                id: asset.base.id,
                sensitivity_class: asset.sensitivity_class.as_str().to_string(),
            }))
    }
}

fn party_list_fact(party: erp_party::Party) -> PartyListFact {
    PartyListFact {
        id: party.base.id,
        party_no: party.party_no,
        status: map_party_status(party.stable.status),
        unified_credit_code: party.unified_credit_code,
        version: party.base.version,
        current_revision_id: party.stable.current_revision_id,
    }
}

fn party_revision_fact(revision: erp_party::PartyRevision) -> PartyRevisionFact {
    PartyRevisionFact {
        id: revision.base.id,
        party_id: revision.party_id.to_string(),
        legal_name: revision.legal_name,
        short_name: revision.short_name,
    }
}

fn party_contact_fact(contact: erp_party::PartyContact) -> PartyContactFact {
    let view = erp_party::PartyContactView::from(contact);
    PartyContactFact {
        id: view.id,
        party_id: view.party_id,
        contact_name: view.contact_name,
        title: view.title,
        telephone: view.telephone,
        mobile_masked: view.mobile_masked,
        email: view.email,
        valid_from: view.valid_from,
        valid_to: view.valid_to,
        is_default: view.is_default,
        status: map_record_status(view.status),
        version: view.version,
        created_at: view.created_at,
    }
}

fn party_address_fact(address: erp_party::PartyAddress) -> PartyAddressFact {
    let view = erp_party::PartyAddressView::from(address);
    PartyAddressFact {
        id: view.id,
        party_id: view.party_id,
        address_type: map_address_type(view.address_type),
        contact_name: view.contact_name,
        valid_from: view.valid_from,
        valid_to: view.valid_to,
        is_default: view.is_default,
        status: map_record_status(view.status),
        version: view.version,
        created_at: view.created_at,
    }
}

fn party_tax_profile_fact(profile: erp_party::PartyTaxProfile) -> PartyTaxProfileFact {
    let view = erp_party::PartyTaxProfileView::from(profile);
    PartyTaxProfileFact {
        id: view.id,
        party_id: view.party_id,
        tax_no: view.tax_no,
        valid_from: view.valid_from,
        valid_to: view.valid_to,
        is_default: view.is_default,
        status: map_record_status(view.status),
        version: view.version,
        created_at: view.created_at,
    }
}

fn party_bank_account_fact(account: erp_party::PartyBankAccount) -> PartyBankAccountFact {
    let view = erp_party::PartyBankAccountView::from(account);
    PartyBankAccountFact {
        id: view.id,
        bank_account_no: view.bank_account_no,
        party_id: view.party_id,
        account_name: view.account_name,
        bank_name: view.bank_name,
        account_number_masked: view.account_number_masked,
        bank_branch_name: view.bank_branch_name,
        valid_from: view.valid_from,
        valid_to: view.valid_to,
        is_default: view.is_default,
        status: map_record_status(view.status),
        version: view.version,
        created_at: view.created_at,
    }
}

fn map_party_status(status: erp_party::PartyStatus) -> PartyStatusFact {
    match status {
        erp_party::PartyStatus::Active => PartyStatusFact::Active,
        erp_party::PartyStatus::Disabled => PartyStatusFact::Disabled,
    }
}

fn map_record_status(status: erp_party::EffectiveRecordStatus) -> EffectiveRecordStatusFact {
    match status {
        erp_party::EffectiveRecordStatus::Active => EffectiveRecordStatusFact::Active,
        erp_party::EffectiveRecordStatus::Disabled => EffectiveRecordStatusFact::Disabled,
    }
}

fn map_address_type(address_type: erp_party::AddressType) -> AddressTypeFact {
    match address_type {
        erp_party::AddressType::Registered => AddressTypeFact::Registered,
        erp_party::AddressType::Operating => AddressTypeFact::Operating,
        erp_party::AddressType::Fulfillment => AddressTypeFact::Fulfillment,
    }
}

fn map_sensitive_kind(kind: SensitiveFieldKindFact) -> SensitiveFieldKind {
    match kind {
        SensitiveFieldKindFact::ContactMobile => SensitiveFieldKind::ContactMobile,
        SensitiveFieldKindFact::Address => SensitiveFieldKind::Address,
        SensitiveFieldKindFact::BankAccountNumber => SensitiveFieldKind::BankAccountNumber,
    }
}

fn map_party_to_supplier(error: erp_party::Error) -> erp_supplier::Error {
    match error {
        erp_party::Error::Internal(message) => erp_supplier::Error::Internal(message),
        erp_party::Error::NotFound(message) => erp_supplier::Error::NotFound(message),
        erp_party::Error::ValidationError(message) => erp_supplier::Error::ValidationError(message),
        erp_party::Error::BusinessLogicError(message) => erp_supplier::Error::BusinessLogicError(message),
        erp_party::Error::ConflictError(message) => erp_supplier::Error::ConflictError(message),
        erp_party::Error::ReceiptDuplicate(error) => erp_supplier::Error::ReceiptDuplicate(error),
        erp_party::Error::TransientTransaction(error) => erp_supplier::Error::TransientTransaction(error),
        erp_party::Error::Forbidden(message) => erp_supplier::Error::Forbidden(message),
        erp_party::Error::Unauthenticated(message) => erp_supplier::Error::Unauthenticated(message),
        erp_party::Error::Logic(error) => erp_supplier::Error::Logic(error),
        erp_party::Error::OutcomeUnknown(error) => erp_supplier::Error::OutcomeUnknown(error),
        erp_party::Error::RepositoryError(error) => erp_supplier::Error::RepositoryError(error),
    }
}

/// 供应商查询读取账号显示名与登录资格。
#[derive(Clone)]
pub struct MongoSupplierAccountFacts {
    db: Database,
}

impl MongoSupplierAccountFacts {
    /// 绑定身份账号集合。
    ///
    /// # 参数
    /// * `db` - 身份集合所在数据库
    ///
    /// # 返回
    /// 返回未执行 I/O 的 adapter。
    ///
    /// # 错误
    /// 无。
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// 包装为供应商域可注入的账号 Port。
    ///
    /// # 参数
    /// * `db` - 身份集合所在数据库
    ///
    /// # 返回
    /// 返回账号事实 Port。
    ///
    /// # 错误
    /// 无。
    pub fn shared(db: Database) -> Arc<dyn AccountFactPort> {
        Arc::new(Self::new(db))
    }
}

#[async_trait]
impl AccountFactPort for MongoSupplierAccountFacts {
    async fn ensure_can_login(&self, user_id: &str) -> erp_supplier::Result<()> {
        let account = self
            .db
            .accounts()
            .find_account(user_id, &mut NoTransaction)
            .await
            .map_err(erp_supplier::Error::from)?
            .ok_or_else(|| erp_supplier::Error::NotFound("维护人账号不存在".to_string()))?;
        account.ensure_can_login().map_err(|error| erp_supplier::Error::BusinessLogicError(error.to_string()))
    }

    async fn names_by_ids(&self, account_ids: &[String]) -> erp_supplier::Result<HashMap<String, String>> {
        self.db
            .accounts()
            .names_by_ids(account_ids, &mut NoTransaction)
            .await
            .map_err(erp_supplier::Error::from)
    }
}
