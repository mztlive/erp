//! 只按真实稳定资金身份、原银行付款和唯一原使用匹配，不使用余额差或名称。

use erp_core::money::Amount;
use mongodb::Database;
use mongodb::bson::doc;
use mongodb::options::FindOptions;
use persistence_core::{Executor, mongo_ops};

use crate::entity::payable::SupplierPayment;
use crate::entity::supplier_deposit::{
    DepositLedgerKind, DepositRechargeStatus, SupplierDepositLedgerEntry, SupplierDepositRechargeRequest,
    SupplierDepositUsage,
};
use crate::repository::{PayableExt, SupplierDepositExt};
use crate::{Error, Result};

/// 读取账户稳定外部资金交易的唯一既有记账结果。
/// # 参数
/// * `db` / `account` / `transaction` / `executor` - 已绑定账号与来源交易身份。
/// # 返回
/// 返回原流水；不存在不代表未扣款。
/// # 错误
/// 数据库查询失败时拒绝。
pub async fn ledger_by_external_transaction(
    db: &Database,
    account: &str,
    transaction: &str,
    executor: &mut dyn Executor,
) -> Result<Option<SupplierDepositLedgerEntry>> {
    Ok(db
        .supplier_deposit_ledger()
        .find_one(doc! {"account_id": account, "evidence.external_transaction_id": transaction}, executor)
        .await?)
}

/// 将实际到账充值匹配唯一已批准且银行已付的原申请。
/// # 参数
/// * `db` / `account` / `reference` / `amount` / `executor` - 真实银行及到账依据。
/// # 返回
/// 只有唯一付款和原申请金额完全相符才返回候选。
/// # 错误
/// 多个原付款、查询失败或来源损坏时拒绝，不按远端余额补造申请。
pub async fn bank_paid_recharge_candidate(
    db: &Database,
    account: &str,
    reference: &str,
    amount: Amount,
    executor: &mut dyn Executor,
) -> Result<Option<SupplierDepositRechargeRequest>> {
    let rows: Vec<SupplierPayment> = mongo_ops::find_many(&db.supplier_payments().collection(), doc! {"purpose.type": "supplier_deposit_recharge", "purpose.deposit_account_id": account, "bank_reference": reference, "status": "posted", "deleted_at": 0_i64}, FindOptions::builder().limit(2).build(), executor).await?;
    if rows.len() > 1 {
        return Err(Error::ConflictError("充值银行流水对应多个付款，请核对原始证据".into()));
    }
    let Some(payment) = rows.into_iter().next() else {
        return Ok(None);
    };
    if payment.effective_amount() != amount {
        return Ok(None);
    }

    let request = db
        .supplier_deposit_recharge_requests()
        .find_one(doc! {"account_id": account, "supplier_payment_id": &payment.base.id}, executor)
        .await?;
    Ok(request
        .filter(|request| request.status == DepositRechargeStatus::BankPaid && request.amount == amount))
}

/// 返款只关联稳定原扣款或同外单唯一原使用，多次扣款不能猜测分摊。
/// # 参数
/// * `db` / `account` / `external_order` / `original_transaction` / `executor` - 原账号、订单和可选来源原交易。
/// # 返回
/// 返回唯一原真实扣款使用；缺失或歧义保持待核对。
/// # 错误
/// 来源损坏或查询失败时拒绝。
pub async fn unique_original_usage(
    db: &Database,
    account: &str,
    external_order: &str,
    original_transaction: Option<&str>,
    executor: &mut dyn Executor,
) -> Result<Option<SupplierDepositUsage>> {
    let rows: Vec<SupplierDepositLedgerEntry> = if let Some(transaction) = original_transaction {
        ledger_by_external_transaction(db, account, transaction, executor).await?.into_iter().collect()
    } else {
        mongo_ops::find_many(&db.supplier_deposit_ledger().collection(), doc! {"account_id": account, "kind": "debit", "evidence.external_order_id": external_order, "deleted_at": 0_i64}, FindOptions::builder().limit(2).build(), executor).await?
    };
    let [entry] = rows.as_slice() else {
        return Ok(None);
    };
    if entry.kind != DepositLedgerKind::Debit {
        return Ok(None);
    }

    let usage = db.supplier_deposit_usages().find_by_id(&entry.base.id, executor).await?;
    Ok(usage.filter(|usage| usage.account_id == account && usage.ledger_entry_id == entry.base.id))
}
