use crate::entity::supplier::{
    InvoiceType, ReconciliationCycle, SettlementMode, SupplierAccount, SupplierAccountUpdate,
    SupplierCommercialProfileRevision, SupplierCommercialProfileRevisionData,
};
use erp_core::field_update::FieldUpdate;
use erp_core::ids::{PartyId, SupplierAccountId, SupplierCommercialProfileRevisionId};

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
    pub invoice_tax_rate: Option<erp_core::money::Rate>,
    /// 常用进项税率；None 读取旧单值，Some([]) 明确表示未登记。
    pub invoice_tax_rates: Option<Vec<erp_core::money::Rate>>,
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
) -> erp_core::Result<SupplierCommercialProfileRevision> {
    let PlanCommercialProfileRevisionParams {
        supplier,
        settlement_mode,
        reconciliation_cycle,
        payment_term_snapshot,
        business_category,
        invoice_type,
        invoice_tax_rate,
        invoice_tax_rates,
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
            invoice_tax_rates,
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
