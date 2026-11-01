//! 回款候选读取及事务内额度锁定，统一使用完整子账事实。

use std::collections::BTreeSet;

use erp_core::ids::{ReceivableAccountId, ReceivableEntryId};
use mongodb::Database;
use persistence_core::Executor;

use crate::entity::receivable::receipt_capacity::{ReceiptAccountCapacity, ReceiptCapacity};
use crate::entity::receivable::{CustomerReceipt, EntryDirection, ReceivableAccount};
use crate::repository::ReceivableExt;
use crate::repository::prelude::*;
use crate::{Error, Result};

/// 读取已授权子账的当前核销额度，不写入也不另开事务。
///
/// # 参数
/// * `db` - 财务数据库。
/// * `account_ids` - 调用方已授权的子账。
/// * `executor` - 调用方快照事务。
///
/// # 返回
/// 扣除已核销、冲减、审批占用并受子账余额限制的项目额度。
///
/// # 错误
/// 查询或金额运算失败时返回错误。
pub async fn receipt_capacity(
    db: &Database,
    account_ids: &[String],
    executor: &mut dyn Executor,
) -> Result<ReceiptCapacity> {
    let accounts = db.receivable_accounts().find_accounts_by_ids(account_ids, executor).await?;
    load(db, accounts, None, executor).await
}

/// 写锁全部来源子账后重验额度；新提交及最终核销共用本入口。
///
/// # 参数
/// * `db` - 财务数据库。
/// * `receipt` - 当前回款及本次全部拟核销明细。
/// * `executor` - 调用方写事务，失败须整体回滚。
///
/// # 返回
/// 锁定子账并确认扣除其他回款占用后仍足额时成功。
///
/// # 错误
/// 来源缺失、主体不符、余额不足或并发写冲突时返回错误。
pub async fn reserve_receipt(
    db: &Database,
    receipt: &CustomerReceipt,
    executor: &mut dyn Executor,
) -> Result<()> {
    let entry_ids =
        receipt.pending_allocations.iter().map(|line| line.receivable_entry_id.clone()).collect::<Vec<_>>();
    let entries = db.receivable_entries().find_entries_by_ids(&entry_ids, executor).await?;
    let account_ids =
        entries.iter().map(|entry| entry.receivable_account_id.to_string()).collect::<BTreeSet<_>>();
    let mut accounts = Vec::with_capacity(account_ids.len());
    for id in account_ids {
        let mut account = db
            .receivable_accounts()
            .find_by_id(&id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("应收子账不存在".into()))?;
        // 与其他回款提交、实际核销及销售应收变更竞争同一文档的 CAS 写入。
        db.receivable_accounts().update(&mut account, executor).await?;
        accounts.push(account);
    }
    load(db, accounts, Some(&receipt.base.id), executor).await?.validate(receipt)
}

async fn load(
    db: &Database,
    accounts: Vec<ReceivableAccount>,
    excluded_receipt: Option<&str>,
    executor: &mut dyn Executor,
) -> Result<ReceiptCapacity> {
    let account_ids =
        accounts.iter().map(|account| ReceivableAccountId::new(&account.base.id)).collect::<Vec<_>>();
    let entries = db.receivable_entries().find_entries_by_accounts(&account_ids, executor).await?;
    let ids = entries.iter().map(|entry| ReceivableEntryId::new(&entry.base.id)).collect::<Vec<_>>();
    let allocations = db.receipt_allocations().find_allocations_by_entries(&ids, executor).await?;
    let decreases = entries
        .iter()
        .filter(|entry| entry.direction == EntryDirection::Decrease)
        .map(|entry| ReceivableEntryId::new(&entry.base.id))
        .collect::<Vec<_>>();
    let offsets = db.receivable_entry_offsets().find_offsets_by_decreases(&decreases, executor).await?;
    let reserved = db.customer_receipts().reserved_allocations(&ids, excluded_receipt, executor).await?;
    let accounts = accounts
        .into_iter()
        .map(|account| ReceiptAccountCapacity {
            id: account.base.id,
            party_id: account.counterparty_party_id,
            open: account.open_total,
        })
        .collect();
    ReceiptCapacity::new(accounts, &entries, &allocations, &offsets, &reserved)
}
