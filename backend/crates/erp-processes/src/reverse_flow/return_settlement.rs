//! 采购处置方案与财务退款、留抵记录的限额及入账关系。
use entity_core::BaseModel;
use erp_core::money::Amount;
use erp_finance::entity::payable::supplier_credit::SupplierCredit;
use erp_finance::entity::payable::{PayableAccount, SupplierPayment};
use erp_finance::repository::PayableExt;
use erp_finance::repository::prelude::*;
use erp_finance::repository::supplier_credit::supplier_credits;
use erp_procurement::repository::PurchaseOrderExt;
use erp_procurement::repository::purchase_source_update::find_open_for_purchase;
use erp_read_models::purchase_center::direct_return::direct_return_plan;
use erp_returns::entity::returns::{SupplierRefund, SupplierSettlementMethod};
use erp_returns::repository::direct_return::purchase_settlements;
use mongodb::Database;
use persistence_core::Executor;

use crate::{Error, Result};

/// 在财务提交和最终过账时复核采购方案、金额与实际退回。
/// # 参数
/// * `db` / `executor` - 原事务。
/// * `refund` - 本次现金或留抵处理记录。
/// # 返回
/// 指定来源采购的应付；历史普通退款返回空。
/// # 错误
/// 未提交方案、来源变化、跨供应商或超过选定方式金额时拒绝。
pub(super) async fn validate_settlement(
    db: &Database,
    refund: &SupplierRefund,
    executor: &mut dyn Executor,
) -> Result<Option<PayableAccount>> {
    let Some(id) = refund.source_purchase_order_id.as_deref() else {
        if refund.settlement_method == SupplierSettlementMethod::SupplierCredit {
            return Err(Error::ValidationError("留抵必须关联采购提交的退货处置方案".into()));
        }
        return Ok(None);
    };
    let update = find_open_for_purchase(db, id, executor)
        .await?
        .ok_or_else(|| Error::ConflictError("采购依据更新已结束".into()))?;
    let decision = update
        .return_settlement
        .ok_or_else(|| Error::ConflictError("请先由采购确认退货款处置方案".into()))?;
    let mut order = db
        .purchase_orders()
        .find_by_id(id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("来源采购不存在".into()))?;
    let plan = direct_return_plan(db, &order, executor).await?;
    if refund.settlement_decision_key.as_deref() != Some(decision.idempotency_key.as_str())
        || order.supplier_id != refund.supplier_id
        || plan.sales_revision_id != decision.sales_revision_id
        || plan.target_gross != decision.target_gross
        || plan.lines.iter().any(|line| !line.required_return.to_decimal().is_zero())
    {
        return Err(Error::ConflictError("来源或退货结果已变化，请重新核对采购方案".into()));
    }
    let limit = if refund.settlement_method == SupplierSettlementMethod::SupplierCredit {
        decision.credit_amount
    } else {
        decision.refund_amount
    };
    let used = purchase_settlements(db, id, executor)
        .await?
        .into_iter()
        .filter(|row| {
            row.base.id != refund.base.id
                && row.settlement_method == refund.settlement_method
                && row.settlement_decision_key == refund.settlement_decision_key
        })
        .fold(Amount::zero(), |total, row| total.checked_add(row.amount));
    if refund.amount > limit.checked_sub(used) {
        return Err(Error::ConflictError("本次金额超过采购方案剩余可处理金额，审批中的金额也已占用".into()));
    }
    // 同采购所有处置申请、退回和销售依据变化竞争同一 CAS。
    db.purchase_orders().update(&mut order, executor).await?;
    let account = db
        .payable_accounts()
        .find_by_purchase_order(&id.to_owned().into(), executor)
        .await?
        .ok_or_else(|| Error::NotFound("采购应付不存在".into()))?;
    Ok(Some(account))
}

/// 审批通过后建立留抵余额；该操作不生成现金退款或新付款。
/// # 参数
/// * `db` / `executor` - 财务过账事务。
/// * `refund` - 已审批的留抵意图。
/// * `payment` - 原实际付款。
/// * `account` - 经本次事务复核的原应付。
/// # 返回
/// 一份来源唯一、尚未使用的余额。
/// # 错误
/// 付款主体缺失或与应付主体不一致时拒绝。
pub(super) async fn create_credit(
    db: &Database,
    refund: &SupplierRefund,
    payment: &SupplierPayment,
    account: &PayableAccount,
    executor: &mut dyn Executor,
) -> Result<()> {
    let payer = payment
        .payer_party_id
        .as_ref()
        .filter(|id| !id.is_empty())
        .ok_or_else(|| Error::ConflictError("原付款缺少我方付款主体，不能转留抵".into()))?;
    if account.settlement_party_id.as_ref() != Some(payer) {
        return Err(Error::ConflictError("原付款与采购的付款主体不一致".into()));
    }
    let record = SupplierCredit {
        base: BaseModel::new(format!("SC-{}", refund.base.id)),
        source_refund_id: refund.base.id.clone(),
        source_purchase_order_id: account.source_document_id.clone(),
        original_payment_id: payment.base.id.clone(),
        supplier_id: refund.supplier_id.to_string(),
        payer_party_id: payer.clone(),
        amount: refund.amount,
        used_amount: Amount::zero(),
    };
    supplier_credits(db).create(&record, executor).await?;
    Ok(())
}

/// 冻结本次财务审批引用的供应商确认依据。
/// # 参数
/// * `db` / `executor` - 提交事务。
/// * `refund` - 已按当前方案校验的处置记录。
/// # 返回
/// 记录留存提交时的确认依据，后续采购调整不会覆盖。
/// # 错误
/// 来源方案缺失时拒绝。
pub(super) async fn freeze_confirmation(
    db: &Database,
    refund: &mut SupplierRefund,
    executor: &mut dyn Executor,
) -> Result<()> {
    if let Some(id) = refund.source_purchase_order_id.as_deref() {
        let decision = find_open_for_purchase(db, id, executor)
            .await?
            .and_then(|update| update.return_settlement)
            .ok_or_else(|| Error::ConflictError("采购处置方案不存在".into()))?;
        refund.settlement_confirmation = Some(decision.supplier_confirmation);
    }
    Ok(())
}
