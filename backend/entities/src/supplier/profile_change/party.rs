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

/// 创建主体名称新修订并更新统一社会信用代码。
///
/// # 参数
/// * `party` - 待修订的主体实体，已通过版本与启停门禁；方法内原地更新 `unified_credit_code` 并推进 `current_revision_id`
/// * `unified_credit_code` - 统一社会信用代码输入，`Some` 表示设置、`None` 表示清空
/// * `legal_name` - 法定名称
/// * `short_name` - 简称
/// * `change_reason` - 修订原因
/// * `revision_id` - 新修订主键，由 Service 分配
/// * `revision_no` - 新修订序号，由 Service 查询得出
/// * `actor_id` - 操作人 ID
///
/// # 返回
/// 返回新建的 `PartyRevision`，并已推进 `party.current_revision_id`。
///
/// # 错误
/// 统一社会信用代码格式非法、法定名称校验失败或修订号越界时返回错误。
///
/// # 约束
/// 纯内存操作，不触及 MongoDB、全局 ID 或加密；`party_no` 与 `party_kind` 不在此修改。
#[allow(clippy::too_many_arguments)]
pub fn plan_party_revision(
    party: &mut Party,
    unified_credit_code: Option<String>,
    legal_name: String,
    short_name: Option<String>,
    change_reason: String,
    revision_id: PartyRevisionId,
    revision_no: u32,
    actor_id: &str,
) -> crate::Result<PartyRevision> {
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

/// 创建商务资料新修订并推进供应商当前指针。
///
/// # 参数
/// * `supplier` - 待修订的供应商实体，已通过版本门禁；原地推进 `current_commercial_profile_revision_id`
/// * `settlement_mode` - 结算方式
/// * `reconciliation_cycle` - 对账周期
/// * `payment_term_snapshot` - 付款条件快照
/// * `business_category` - 经营类目
/// * `invoice_type` - 发票类型
/// * `invoice_tax_rate` - 发票税点
/// * `signing_entity_party_id` - 签约主体
/// * `payment_entity_party_id` - 付款主体
/// * `change_reason` - 变更原因
/// * `revision_id` - 新修订主键
/// * `revision_no` - 新修订序号
/// * `actor_id` - 操作人 ID
///
/// # 返回
/// 返回新建的商务资料修订，并已推进 `supplier` 指针。
///
/// # 错误
/// 付款条件、税点或变更原因非法时返回错误。
///
/// # 约束
/// 纯内存，不触及外部 I/O；不分配新 ID，需 Service 注入。
#[allow(clippy::too_many_arguments)]
pub fn plan_commercial_profile_revision(
    supplier: &mut SupplierAccount,
    settlement_mode: SettlementMode,
    reconciliation_cycle: ReconciliationCycle,
    payment_term_snapshot: String,
    business_category: Option<String>,
    invoice_type: InvoiceType,
    invoice_tax_rate: crate::money::Rate,
    signing_entity_party_id: PartyId,
    payment_entity_party_id: PartyId,
    change_reason: String,
    revision_id: SupplierCommercialProfileRevisionId,
    revision_no: u32,
    actor_id: &str,
) -> crate::Result<SupplierCommercialProfileRevision> {
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
