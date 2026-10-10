//! 单张采购单的有效付款核销与回单，不返回其他采购单分配或整笔合并付款金额。

use erp_core::common::time::Instant;
use erp_core::ids::{PayableAccountId, PurchaseOrderId, SupplierPaymentId};
use erp_core::money::Amount;
use erp_finance::entity::payable::{SupplierPayment, SupplierPaymentStatus};
use erp_finance::repository::PayableExt;
use erp_finance::repository::prelude::*;
use erp_support::FileAssetExt;
use mongodb::Database;
use persistence_core::Executor;
use serde::Serialize;

use super::document_files::{FinancialFileView, effective_payment_totals, readable_file};
use crate::{Error, Result};

/// 采购详情内可见的付款事实；金额仅属于当前采购单。
#[derive(Debug, Clone, Serialize)]
pub struct PurchasePaymentView {
    /// 付款单身份。
    pub payment_id: String,
    /// 付款业务单号。
    pub payment_no: String,
    /// 实际付款时间。
    pub paid_at: Instant,
    /// 当前采购单的净核销金额。
    pub allocated_amount: Amount,
    /// 可读取的银行回单；历史未上传或文件隔离时为空。
    pub receipt: Option<FinancialFileView>,
}

/// 调用方须先证明采购来源资格，并传入同一事务执行器。
/// # 参数
/// 数据库、已授权采购单及调用方执行器。
/// # 返回
/// 返回当前采购单的有效付款净核销与可读回单。
/// # 错误
/// 关联查询失败或回单事实缺失时拒绝。
pub(crate) async fn payment_records(
    db: &Database,
    purchase_id: &PurchaseOrderId,
    executor: &mut dyn Executor,
) -> Result<Vec<PurchasePaymentView>> {
    let Some(account) = db.payable_accounts().find_by_purchase_order(purchase_id, executor).await? else {
        return Ok(vec![]);
    };
    let entries = db
        .payable_entries()
        .find_entries_by_accounts(&[PayableAccountId::new(account.base.id)], executor)
        .await?;
    let ids = entries.into_iter().map(|e| e.base.id.into()).collect::<Vec<_>>();
    let allocations = db.payment_allocations().find_allocations_by_entries(&ids, executor).await?;
    let totals = effective_payment_totals(&allocations);
    let ids = totals.keys().map(|id| SupplierPaymentId::new(id.clone())).collect::<Vec<_>>();
    let payments = db.supplier_payments().find_supplier_payments_by_ids(&ids, executor).await?;
    let mut result = vec![];
    for payment in payments.into_iter().filter(|p| p.status == SupplierPaymentStatus::Posted) {
        let Some(amount) = totals.get(&payment.base.id) else { continue };
        let receipt = receipt_file(db, &payment, executor).await?;
        result.push(PurchasePaymentView {
            payment_id: payment.base.id,
            payment_no: payment.payment_no,
            paid_at: payment.paid_at,
            allocated_amount: *amount,
            receipt,
        });
    }
    result.sort_by(|a, b| b.paid_at.cmp(&a.paid_at).then_with(|| a.payment_id.cmp(&b.payment_id)));
    Ok(result)
}

async fn receipt_file(
    db: &Database,
    payment: &SupplierPayment,
    executor: &mut dyn Executor,
) -> Result<Option<FinancialFileView>> {
    let Some(id) = &payment.bank_receipt_asset_id else { return Ok(None) };
    let file = db
        .file_assets()
        .find_by_id(id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("付款回单不存在".into()))?;
    Ok(readable_file(&file)
        .then(|| FinancialFileView::new(payment.base.id.clone(), payment.payment_no.clone(), file)))
}
