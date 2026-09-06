//! Convert party entities into customer HTTP snapshots.

use erp_customer::{
    AddressType, EffectiveRecordStatus, PartyAddressView, PartyBankAccountView, PartyContactView,
    PartyRevisionView, PartyStatus, PartyTaxProfileView, SensitiveFieldKind,
};
use erp_party::{
    PartyAddress, PartyBankAccount, PartyContact, PartyRevision, PartyTaxProfile,
    SensitiveFieldKind as PartySensitiveFieldKind,
};

pub fn party_status(status: erp_party::PartyStatus) -> PartyStatus {
    match status {
        erp_party::PartyStatus::Active => PartyStatus::Active,
        erp_party::PartyStatus::Disabled => PartyStatus::Disabled,
    }
}

pub fn party_revision_view(revision: PartyRevision) -> PartyRevisionView {
    let view = erp_party::PartyRevisionView::from(revision);
    PartyRevisionView {
        id: view.id,
        revision_no: view.revision_no,
        legal_name: view.legal_name,
        short_name: view.short_name,
        change_reason: view.change_reason,
        version: view.version,
        created_at: view.created_at,
    }
}

pub fn party_contact_view(contact: PartyContact) -> PartyContactView {
    let view = erp_party::PartyContactView::from(contact);
    PartyContactView {
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
        status: record_status(view.status),
        version: view.version,
        created_at: view.created_at,
    }
}

pub fn party_address_view(address: PartyAddress) -> PartyAddressView {
    let view = erp_party::PartyAddressView::from(address);
    PartyAddressView {
        id: view.id,
        party_id: view.party_id,
        address_type: address_type(view.address_type),
        contact_name: view.contact_name,
        valid_from: view.valid_from,
        valid_to: view.valid_to,
        is_default: view.is_default,
        status: record_status(view.status),
        version: view.version,
        created_at: view.created_at,
    }
}

pub fn party_tax_profile_view(profile: PartyTaxProfile) -> PartyTaxProfileView {
    let view = erp_party::PartyTaxProfileView::from(profile);
    PartyTaxProfileView {
        id: view.id,
        party_id: view.party_id,
        tax_no: view.tax_no,
        valid_from: view.valid_from,
        valid_to: view.valid_to,
        is_default: view.is_default,
        status: record_status(view.status),
        version: view.version,
        created_at: view.created_at,
    }
}

pub fn party_bank_account_view(account: PartyBankAccount) -> PartyBankAccountView {
    let view = erp_party::PartyBankAccountView::from(account);
    PartyBankAccountView {
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
        status: record_status(view.status),
        version: view.version,
        created_at: view.created_at,
    }
}

pub fn customer_sensitive_kind(kind: PartySensitiveFieldKind) -> SensitiveFieldKind {
    match kind {
        PartySensitiveFieldKind::ContactMobile => SensitiveFieldKind::ContactMobile,
        PartySensitiveFieldKind::Address => SensitiveFieldKind::Address,
        PartySensitiveFieldKind::BankAccountNumber => SensitiveFieldKind::BankAccountNumber,
    }
}

fn record_status(status: erp_party::EffectiveRecordStatus) -> EffectiveRecordStatus {
    match status {
        erp_party::EffectiveRecordStatus::Active => EffectiveRecordStatus::Active,
        erp_party::EffectiveRecordStatus::Disabled => EffectiveRecordStatus::Disabled,
    }
}

pub fn party_address_type(address_type: AddressType) -> erp_party::AddressType {
    match address_type {
        AddressType::Registered => erp_party::AddressType::Registered,
        AddressType::Operating => erp_party::AddressType::Operating,
        AddressType::Fulfillment => erp_party::AddressType::Fulfillment,
    }
}

fn address_type(address_type: erp_party::AddressType) -> AddressType {
    match address_type {
        erp_party::AddressType::Registered => AddressType::Registered,
        erp_party::AddressType::Operating => AddressType::Operating,
        erp_party::AddressType::Fulfillment => AddressType::Fulfillment,
    }
}
