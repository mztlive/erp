//! 把主体实体映射为客户资料返回快照。

use erp_customer::{
    AddressType, EffectiveRecordStatus, PartyAddressView, PartyBankAccountView, PartyContactView,
    PartyRevisionView, PartyStatus, PartyTaxProfileView, SensitiveFieldKind,
};
use erp_party::{
    PartyAddress, PartyBankAccount, PartyContact, PartyRevision, PartyTaxProfile,
    SensitiveFieldKind as PartySensitiveFieldKind,
};

/// 把主体状态映射为客户资料状态。
///
/// # 参数
/// * `status` - 主体稳定状态。
///
/// # 返回
/// 返回同名的客户资料状态。
///
/// # 错误
/// 不返回错误。
pub fn party_status(status: erp_party::PartyStatus) -> PartyStatus {
    match status {
        erp_party::PartyStatus::Active => PartyStatus::Active,
        erp_party::PartyStatus::Disabled => PartyStatus::Disabled,
    }
}

/// 把主体名称修订映射为客户资料修订快照。
///
/// # 参数
/// * `revision` - 主体名称修订。
///
/// # 返回
/// 返回不含敏感明文的修订视图。
///
/// # 错误
/// 不返回错误。
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

/// 把联系人事实映射为客户资料联系人快照，手机号只保留掩码。
///
/// # 参数
/// * `contact` - 联系人事实。
///
/// # 返回
/// 返回联系人视图。
///
/// # 错误
/// 不返回错误。
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

/// 把地址事实映射为客户资料地址快照，不输出地址明文。
///
/// # 参数
/// * `address` - 地址事实。
///
/// # 返回
/// 返回地址视图。
///
/// # 错误
/// 不返回错误。
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

/// 把税务资料映射为客户资料税务快照。
///
/// # 参数
/// * `profile` - 税务资料事实。
///
/// # 返回
/// 返回税务资料视图。
///
/// # 错误
/// 不返回错误。
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

/// 把银行账户映射为客户资料账户快照，账号只保留掩码。
///
/// # 参数
/// * `account` - 银行账户事实。
///
/// # 返回
/// 返回银行账户视图。
///
/// # 错误
/// 不返回错误。
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

/// 把主体敏感字段种类映射为客户资料种类。
///
/// # 参数
/// * `kind` - 主体侧敏感字段种类。
///
/// # 返回
/// 返回同名的客户资料敏感字段种类。
///
/// # 错误
/// 不返回错误。
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

/// 把客户资料地址类型映射为主体地址类型。
///
/// # 参数
/// * `address_type` - 客户资料地址类型。
///
/// # 返回
/// 返回同名的主体地址类型。
///
/// # 错误
/// 不返回错误。
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
