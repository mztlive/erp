use crate::field_update::FieldUpdate;
use crate::ids::{PartyId, PartyRevisionId, SupplierAccountId, SupplierCommercialProfileRevisionId};
use crate::party::{
    Party, PartyAddress, PartyAddressUpdate, PartyBankAccount, PartyBankAccountUpdate, PartyContact,
    PartyContactUpdate, PartyRevision, PartyRevisionData, PartyTaxProfile, PartyTaxProfileUpdate,
    PartyUpdate,
};
use crate::supplier::{
    InvoiceType, ReconciliationCycle, SettlementMode, SupplierAccount, SupplierAccountUpdate,
    SupplierCommercialProfileRevision, SupplierCommercialProfileRevisionData,
};

use super::types::option_as_authoritative_update;

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

/// 创建主体名称新修订并更新统一社会信用代码。
///
/// # 参数
/// * `params` - 主体、信用代码、名称与修订身份
///
/// # 返回
/// 返回新建的 `PartyRevision`，并已推进 `party.current_revision_id`。
///
/// # 错误
/// 统一社会信用代码格式非法、法定名称校验失败或修订号越界时返回错误。
///
/// # 约束
/// 纯内存操作，不触及 MongoDB、全局 ID 或加密；`party_no` 与 `party_kind` 不在此修改。
pub fn plan_party_revision(params: PlanPartyRevisionParams<'_>) -> crate::Result<PartyRevision> {
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

/// 创建商务资料新修订的输入参数。
#[derive(Debug)]
pub struct PlanCommercialProfileRevisionParams<'a> {
    /// 待修订的供应商实体，已通过版本门禁；原地推进 `current_commercial_profile_revision_id`。
    pub supplier: &'a mut SupplierAccount,
    /// 结算方式。
    pub settlement_mode: SettlementMode,
    /// 对账周期。
    pub reconciliation_cycle: ReconciliationCycle,
    /// 付款条件快照。
    pub payment_term_snapshot: String,
    /// 经营类目。
    pub business_category: Option<String>,
    /// 发票类型。
    pub invoice_type: InvoiceType,
    /// 发票税点。
    pub invoice_tax_rate: crate::money::Rate,
    /// 签约主体。
    pub signing_entity_party_id: PartyId,
    /// 付款主体。
    pub payment_entity_party_id: PartyId,
    /// 变更原因。
    pub change_reason: String,
    /// 新修订主键。
    pub revision_id: SupplierCommercialProfileRevisionId,
    /// 新修订序号。
    pub revision_no: u32,
    /// 操作人 ID。
    pub actor_id: &'a str,
}

/// 创建商务资料新修订并推进供应商当前指针。
///
/// # 参数
/// * `params` - 供应商、商务条款与修订身份
///
/// # 返回
/// 返回新建的商务资料修订，并已推进 `supplier` 指针。
///
/// # 错误
/// 付款条件、税点或变更原因非法时返回错误。
///
/// # 约束
/// 纯内存，不触及外部 I/O；不分配新 ID，需 Service 注入。
pub fn plan_commercial_profile_revision(
    params: PlanCommercialProfileRevisionParams<'_>,
) -> crate::Result<SupplierCommercialProfileRevision> {
    let PlanCommercialProfileRevisionParams {
        supplier,
        settlement_mode,
        reconciliation_cycle,
        payment_term_snapshot,
        business_category,
        invoice_type,
        invoice_tax_rate,
        signing_entity_party_id,
        payment_entity_party_id,
        change_reason,
        revision_id,
        revision_no,
        actor_id,
    } = params;
    let revision = SupplierCommercialProfileRevision::new(
        revision_id.clone(),
        SupplierCommercialProfileRevisionData {
            supplier_id: SupplierAccountId::new(&supplier.base.id),
            revision_no,
            settlement_mode,
            reconciliation_cycle,
            payment_term_snapshot,
            business_category,
            invoice_type,
            invoice_tax_rate,
            signing_entity_party_id,
            payment_entity_party_id,
            change_reason,
        },
    )?;
    supplier.update(
        SupplierAccountUpdate {
            default_payment_term_id: FieldUpdate::Unchanged,
            current_commercial_profile_revision_id: FieldUpdate::Set(revision_id),
            status: None,
        },
        actor_id,
    )?;
    Ok(revision)
}

/// 停用既有联系人事实行，供新默认事实行接替。
///
/// # 参数
/// * `items` - 已加载的联系人集合，已按仓储返回顺序传入
/// * `actor_id` - 操作人 ID
///
/// # 返回
/// 仅保留原 `Active` 记录并将其置为 `Disabled` 且 `is_default=false`；`Inactive` 已被过滤视为不存在。
///
/// # 错误
/// 状态迁移非法时返回错误；区间倒挂不在本方法校验。
///
/// # 约束
/// 纯内存；保留 Service 侧 `retain(is_active)` 语义，不得改变过滤顺序。
pub fn disable_contacts(items: &mut Vec<PartyContact>, actor_id: &str) -> crate::Result<()> {
    items.retain(PartyContact::is_active);
    for item in items.iter_mut().filter(|item| item.is_active()) {
        item.update(
            PartyContactUpdate {
                status: Some(crate::party::status::EffectiveRecordStatus::Disabled),
                valid_to: FieldUpdate::Unchanged,
                is_default: Some(false),
            },
            actor_id,
        )?;
    }
    Ok(())
}

/// 停用既有地址事实行。
///
/// # 参数
/// * `items` - 已加载的地址集合
/// * `actor_id` - 操作人 ID
///
/// # 返回
/// 仅对 `Active` 记录执行停用，其余过滤。
///
/// # 错误
/// 状态迁移非法时返回错误。
///
/// # 约束
/// 纯内存；与 `disable_contacts` 同构，保持一致的停用语义。
pub fn disable_addresses(items: &mut Vec<PartyAddress>, actor_id: &str) -> crate::Result<()> {
    items.retain(PartyAddress::is_active);
    for item in items.iter_mut().filter(|item| item.is_active()) {
        item.update(
            PartyAddressUpdate {
                status: Some(crate::party::status::EffectiveRecordStatus::Disabled),
                valid_to: FieldUpdate::Unchanged,
                is_default: Some(false),
            },
            actor_id,
        )?;
    }
    Ok(())
}

/// 停用既有税务事实行。
///
/// # 参数
/// * `items` - 已加载的税务档案集合
/// * `actor_id` - 操作人 ID
///
/// # 返回
/// 仅 `Active` 被停用。
///
/// # 错误
/// 状态迁移非法时返回错误。
///
/// # 约束
/// 纯内存；保持原 Service 顺序。
pub fn disable_tax_profiles(items: &mut Vec<PartyTaxProfile>, actor_id: &str) -> crate::Result<()> {
    items.retain(PartyTaxProfile::is_active);
    for item in items.iter_mut().filter(|item| item.is_active()) {
        item.update(
            PartyTaxProfileUpdate {
                status: Some(crate::party::status::EffectiveRecordStatus::Disabled),
                valid_to: FieldUpdate::Unchanged,
                is_default: Some(false),
            },
            actor_id,
        )?;
    }
    Ok(())
}

/// 停用既有银行账户事实行。
///
/// # 参数
/// * `items` - 已加载的银行账户集合
/// * `actor_id` - 操作人 ID
///
/// # 返回
/// 仅 `Active` 被停用。
///
/// # 错误
/// 状态迁移非法时返回错误。
///
/// # 约束
/// 纯内存；不触及加密列，仅切换状态与默认值。
pub fn disable_bank_accounts(items: &mut Vec<PartyBankAccount>, actor_id: &str) -> crate::Result<()> {
    items.retain(PartyBankAccount::is_active);
    for item in items.iter_mut().filter(|item| item.is_active()) {
        item.update(
            PartyBankAccountUpdate {
                status: Some(crate::party::status::EffectiveRecordStatus::Disabled),
                valid_to: FieldUpdate::Unchanged,
                is_default: Some(false),
            },
            actor_id,
        )?;
    }
    Ok(())
}
