use application_core::AuditActor;
use erp_core::AccountKind;
use erp_core::ids::SupplierCommercialProfileRevisionId;

use super::application::ensure_actor;
use super::{CooperationApplication, CooperationResult};
use crate::profile_change::{PlanCommercialProfileRevisionParams, plan_commercial_profile_revision};
use crate::{Error, Result, SupplierAccount, SupplierCommercialProfileRevision};

/// 正式写入计划；不触及已有采购订单的冻结快照。
#[derive(Debug)]
pub struct ConfirmedCooperation {
    pub supplier: SupplierAccount,
    pub profile: SupplierCommercialProfileRevision,
    pub result: CooperationResult,
}

/// 用冻结提交创建商务修订，并保留原商务档案的非申请字段。
///
/// # 参数
/// * `application` - 服务器已授权的待确认申请。
/// * `supplier` - 同事务读取的当前供应商。
/// * `current` - 当前商务指针指向的版本。
/// * `revision_id` - 新修订身份。
/// * `actor` - 真实内部确认人。
/// * `now` - 确认时间。
/// # 返回
/// 返回待持久化的 supplier、profile 和确认结果。
/// # 错误
/// 状态、绑定、当前版本、指针或商务规则不满足时拒绝。
pub fn plan_confirmed_cooperation(
    application: &CooperationApplication,
    supplier: &SupplierAccount,
    current: &SupplierCommercialProfileRevision,
    revision_id: SupplierCommercialProfileRevisionId,
    actor: &AuditActor,
    now: u64,
) -> Result<ConfirmedCooperation> {
    ensure_actor(actor, AccountKind::Admin)?;
    application.ensure_confirmer(actor)?;
    let proposal = &application.pending_submission()?.proposal;
    ensure_current(application, supplier, current)?;
    if revision_id.to_string().trim().is_empty() || revision_id.to_string() == current.base.id {
        return Err(Error::ValidationError("新商务修订身份无效".into()));
    }
    let revision_no = current
        .revision
        .revision_no
        .checked_add(1)
        .ok_or_else(|| Error::ConflictError("商务修订版本超限".into()))?;
    let supplier_version =
        supplier.base.version.checked_add(1).ok_or_else(|| Error::ConflictError("供应商版本超限".into()))?;
    let mut supplier = supplier.clone();
    let profile = plan_commercial_profile_revision(PlanCommercialProfileRevisionParams {
        supplier: &mut supplier,
        settlement_mode: proposal.settlement_mode,
        reconciliation_cycle: proposal.reconciliation_cycle,
        payment_term_snapshot: proposal.payment_term.clone(),
        business_category: current.business_category.clone(),
        invoice_type: current.invoice_type,
        invoice_tax_rate: current.invoice_tax_rate,
        invoice_tax_rates: current.invoice_tax_rates.clone(),
        signing_entity_party_id: current.signing_entity_party_id.clone(),
        payment_entity_party_id: current.payment_entity_party_id.clone(),
        change_reason: proposal.reason.clone(),
        revision_id,
        revision_no,
        actor_id: actor.id(),
    })?;
    let result = CooperationResult {
        profile_id: profile.base.id.clone(),
        profile_revision_no: revision_no,
        supplier_version,
        confirmed_by: actor.id().into(),
        confirmed_at: now,
    };
    Ok(ConfirmedCooperation { supplier, profile, result })
}

/// 供应商须启用且与申请绑定，商务指针和期望版本必须仍是当前值。
fn ensure_current(
    application: &CooperationApplication,
    supplier: &SupplierAccount,
    current: &SupplierCommercialProfileRevision,
) -> Result<()> {
    let proposal = &application.pending_submission()?.proposal;
    if supplier.base.is_deleted()
        || !supplier.stable.status.is_active()
        || supplier.base.id != application.supplier_id
        || current.supplier_id.to_string() != supplier.base.id
    {
        return Err(Error::Forbidden("供应商当前状态不允许确认合作条款".into()));
    }
    if supplier.base.version != proposal.expected_supplier_version
        || current.base.is_deleted()
        || current.base.id != proposal.expected_profile_id
        || supplier.current_commercial_profile_revision_id.as_ref().map(ToString::to_string).as_deref()
            != Some(proposal.expected_profile_id.as_str())
    {
        return Err(Error::ConflictError("供应商商务档案已改变，请退回供应商重新核对提交".into()));
    }
    Ok(())
}
