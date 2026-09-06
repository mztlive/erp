//! Party-side profile change helpers owned by the supplier_profile process.

use erp_core::field_update::FieldUpdate;
use erp_core::ids::{PartyId, PartyRevisionId};
use erp_party::{
    EffectiveRecordStatus, Party, PartyAddress, PartyAddressUpdate, PartyBankAccount, PartyBankAccountUpdate,
    PartyContact, PartyContactUpdate, PartyRevision, PartyRevisionData, PartyTaxProfile,
    PartyTaxProfileUpdate, PartyUpdate,
};

/// 创建主体名称新修订的输入参数。
#[derive(Debug)]
pub struct PlanPartyRevisionParams<'a> {
    /// 待修订的主体实体，已通过版本与启停门禁；方法内原地更新 `unified_credit_code` 并推进 `current_revision_id`。
    pub party: &'a mut Party,
    /// 统一社会信用代码输入，`Some` 表示设置、`None` 表示清空。
    pub unified_credit_code: Option<String>,
    /// 法定名称。
    pub legal_name: String,
    /// 简称。
    pub short_name: Option<String>,
    /// 修订原因。
    pub change_reason: String,
    /// 新修订主键，由 Service 分配。
    pub revision_id: PartyRevisionId,
    /// 新修订序号，由 Service 查询得出。
    pub revision_no: u32,
    /// 操作人 ID。
    pub actor_id: &'a str,
}

/// 将可空输入映射为明确设置或清空意图。
fn option_as_authoritative_update<T>(value: Option<T>) -> FieldUpdate<T> {
    value.map_or(FieldUpdate::Clear, FieldUpdate::Set)
}

/// 创建主体名称新修订并更新统一社会信用代码。
pub fn plan_party_revision(params: PlanPartyRevisionParams<'_>) -> erp_core::Result<PartyRevision> {
    let PlanPartyRevisionParams {
        party,
        unified_credit_code,
        legal_name,
        short_name,
        change_reason,
        revision_id,
        revision_no,
        actor_id,
    } = params;
    party.update(
        PartyUpdate {
            unified_credit_code: option_as_authoritative_update(unified_credit_code),
            status: None,
        },
        actor_id,
    )?;
    party.stable.current_revision_id = Some(revision_id.to_string());
    PartyRevision::new(
        revision_id,
        PartyRevisionData {
            party_id: PartyId::new(&party.base.id),
            revision_no,
            legal_name,
            short_name,
            change_reason,
        },
    )
}

/// 停用既有联系人事实行，供新默认事实行接替。
pub fn disable_contacts(items: &mut Vec<PartyContact>, actor_id: &str) -> erp_core::Result<()> {
    items.retain(PartyContact::is_active);
    for item in items.iter_mut().filter(|item| item.is_active()) {
        item.update(
            PartyContactUpdate {
                status: Some(EffectiveRecordStatus::Disabled),
                valid_to: FieldUpdate::Unchanged,
                is_default: Some(false),
            },
            actor_id,
        )?;
    }
    Ok(())
}

/// 停用既有地址事实行。
pub fn disable_addresses(items: &mut Vec<PartyAddress>, actor_id: &str) -> erp_core::Result<()> {
    items.retain(PartyAddress::is_active);
    for item in items.iter_mut().filter(|item| item.is_active()) {
        item.update(
            PartyAddressUpdate {
                status: Some(EffectiveRecordStatus::Disabled),
                valid_to: FieldUpdate::Unchanged,
                is_default: Some(false),
            },
            actor_id,
        )?;
    }
    Ok(())
}

/// 停用既有税务事实行。
pub fn disable_tax_profiles(items: &mut Vec<PartyTaxProfile>, actor_id: &str) -> erp_core::Result<()> {
    items.retain(PartyTaxProfile::is_active);
    for item in items.iter_mut().filter(|item| item.is_active()) {
        item.update(
            PartyTaxProfileUpdate {
                status: Some(EffectiveRecordStatus::Disabled),
                valid_to: FieldUpdate::Unchanged,
                is_default: Some(false),
            },
            actor_id,
        )?;
    }
    Ok(())
}

/// 停用既有银行账户事实行。
pub fn disable_bank_accounts(items: &mut Vec<PartyBankAccount>, actor_id: &str) -> erp_core::Result<()> {
    items.retain(PartyBankAccount::is_active);
    for item in items.iter_mut().filter(|item| item.is_active()) {
        item.update(
            PartyBankAccountUpdate {
                status: Some(EffectiveRecordStatus::Disabled),
                valid_to: FieldUpdate::Unchanged,
                is_default: Some(false),
            },
            actor_id,
        )?;
    }
    Ok(())
}
