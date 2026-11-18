//! 把主体实体映射为客户资料返回快照。

use erp_customer::{
    AddressType, EffectiveRecordStatus, PartyAddressView, PartyBankAccountView, PartyContactView,
    PartyRevisionView, PartyStatus, PartyTaxProfileView, SensitiveFieldKind,
};
use erp_party::{
    AddressType as SourceAddressType, EffectiveRecordStatus as SourceEffectiveRecordStatus, PartyAddress,
    PartyAddressView as SourcePartyAddressView, PartyBankAccount,
    PartyBankAccountView as SourcePartyBankAccountView, PartyContact,
    PartyContactView as SourcePartyContactView, PartyRevision, PartyRevisionView as SourcePartyRevisionView,
    PartyStatus as SourcePartyStatus, PartyTaxProfile, PartyTaxProfileView as SourcePartyTaxProfileView,
    SensitiveFieldKind as PartySensitiveFieldKind,
};

// 投影客户资料详情字段时，把主体状态映射为客户资料状态。
//
// 调用参数：
// * `status` - 主体稳定状态。
//
// 返回同名的客户资料状态。
pub(super) fn party_status(status: SourcePartyStatus) -> PartyStatus {
    match status {
        SourcePartyStatus::Active => PartyStatus::Active,
        SourcePartyStatus::Disabled => PartyStatus::Disabled,
    }
}

// 生成客户资料详情视图时，把主体名称修订映射为客户资料修订快照。
//
// 调用参数：
// * `revision` - 主体名称修订。
//
// 返回不含敏感明文的修订视图。
pub(super) fn party_revision_view(revision: PartyRevision) -> PartyRevisionView {
    let view = SourcePartyRevisionView::from(revision);
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

// 生成客户资料详情视图时，把联系人事实映射为客户资料联系人快照，手机号只保留掩码。
//
// 调用参数：
// * `contact` - 联系人事实。
//
// 返回联系人视图。
pub(super) fn party_contact_view(contact: PartyContact) -> PartyContactView {
    let view = SourcePartyContactView::from(contact);
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

// 生成客户资料详情视图时，把地址事实映射为客户资料地址快照，不输出地址明文。
//
// 调用参数：
// * `address` - 地址事实。
//
// 返回地址视图。
pub(super) fn party_address_view(address: PartyAddress) -> PartyAddressView {
    let view = SourcePartyAddressView::from(address);
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

// 生成客户资料详情视图时，把税务资料映射为客户资料税务快照。
//
// 调用参数：
// * `profile` - 税务资料事实。
//
// 返回税务资料视图。
pub(super) fn party_tax_profile_view(profile: PartyTaxProfile) -> PartyTaxProfileView {
    let view = SourcePartyTaxProfileView::from(profile);
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

// 生成客户资料详情视图时，把银行账户映射为客户资料账户快照，账号只保留掩码。
//
// 调用参数：
// * `account` - 银行账户事实。
//
// 返回银行账户视图。
pub(super) fn party_bank_account_view(account: PartyBankAccount) -> PartyBankAccountView {
    let view = SourcePartyBankAccountView::from(account);
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

// 投影客户资料详情字段时，把主体敏感字段种类映射为客户资料种类。
//
// 调用参数：
// * `kind` - 主体侧敏感字段种类。
//
// 返回同名的客户资料敏感字段种类。
pub(super) fn customer_sensitive_kind(kind: PartySensitiveFieldKind) -> SensitiveFieldKind {
    match kind {
        PartySensitiveFieldKind::ContactMobile => SensitiveFieldKind::ContactMobile,
        PartySensitiveFieldKind::Address => SensitiveFieldKind::Address,
        PartySensitiveFieldKind::BankAccountNumber => SensitiveFieldKind::BankAccountNumber,
    }
}

fn record_status(status: SourceEffectiveRecordStatus) -> EffectiveRecordStatus {
    match status {
        SourceEffectiveRecordStatus::Active => EffectiveRecordStatus::Active,
        SourceEffectiveRecordStatus::Disabled => EffectiveRecordStatus::Disabled,
    }
}

fn address_type(address_type: SourceAddressType) -> AddressType {
    match address_type {
        SourceAddressType::Registered => AddressType::Registered,
        SourceAddressType::Operating => AddressType::Operating,
        SourceAddressType::Fulfillment => AddressType::Fulfillment,
    }
}
