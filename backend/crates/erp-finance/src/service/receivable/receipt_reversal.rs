//! 回款逆向核销的财务事务内写入与关联销售标识读取；不持有根事务或退货聚合。

use erp_core::common::time::Instant;
use erp_core::ids::{CustomerReceiptId, ReceiptAllocationId, ReceivableEntryId, SalesOrderId};
use erp_core::money::Amount;
use id_generator::next_id;
use mongodb::Database;
use persistence_core::Executor;

use crate::entity::receivable::{
    AllocationAction as ReceivableAllocationAction, CustomerReceipt, CustomerReceiptStatus,
    ReceiptAllocation, ReceiptAllocationData, ReceiptReverseChunk, ReceiptReversePlanRow, ReceivableAccount,
    ReceivableEntry,
};
use crate::repository::ReceivableExt;
use crate::repository::prelude::*;
/// 批量索引 helper 已下沉到 [`crate::service::offset_index`]；此处重导出仅保
/// 持既有 `receipt_reversal::` 引用路径兼容，新代码请直引共享模块。
pub use crate::service::offset_index::{
    OffsetFacts, index_required_by_id, unique_account_ids_for_entries, unique_ids_in_first_seen_order,
};
use crate::{Error, Result};

/// 批量读取应收分录及其账户，缺任一项失败关闭。
///
/// # 参数
/// * `db` - 数据库
/// * `entry_ids` - 冲减块引用的增加分录 ID；可含重复
/// * `executor` - 调用方执行器，须与写入共用事务快照
///
/// # 返回
/// 返回按 ID 索引的分录与账户。
///
/// # 错误
/// 缺任一分录或账户时返回 `NotFound`；仓储失败时传播基础设施错误。
///
/// # 约束
/// 固定两次批量读取，次数不随冲减块数量增长；不执行条件更新。
pub async fn load_receivable_offset_facts(
    db: &Database,
    entry_ids: impl IntoIterator<Item = ReceivableEntryId>,
    executor: &mut dyn Executor,
) -> Result<OffsetFacts<ReceivableEntry, ReceivableAccount>> {
    let unique_entry_ids = unique_ids_in_first_seen_order(entry_ids.into_iter().map(|id| id.to_string()));
    let typed_entry_ids = unique_entry_ids.iter().cloned().map(ReceivableEntryId::new).collect::<Vec<_>>();
    let entries = db.receivable_entries().find_entries_by_ids(&typed_entry_ids, executor).await?;
    let entries = index_required_by_id(
        entries,
        &unique_entry_ids,
        |entry| entry.base.id.clone(),
        |_| Error::NotFound("应收分录不存在".to_string()),
    )?;
    let unique_account_ids = unique_account_ids_for_entries(&entries, &unique_entry_ids, |entry| {
        entry.receivable_account_id.to_string()
    });
    let accounts = db.receivable_accounts().find_accounts_by_ids(&unique_account_ids, executor).await?;
    let accounts = index_required_by_id(
        accounts,
        &unique_account_ids,
        |account| account.base.id.clone(),
        |_| Error::NotFound("应收往来子账不存在".to_string()),
    )?;
    Ok(OffsetFacts { entries, accounts })
}

/// 读取可冲正的正式回款；保留先存在性再状态的错误顺序。
pub async fn load_posted_receipt(
    db: &Database,
    receipt_id: &CustomerReceiptId,
    executor: &mut dyn Executor,
) -> Result<CustomerReceipt> {
    let receipt = db
        .customer_receipts()
        .find_by_id(receipt_id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("原回款不存在".to_string()))?;
    if receipt.status != CustomerReceiptStatus::Posted {
        return Err(Error::BusinessLogicError("只有已过账回款可以冲正".to_string()));
    }
    Ok(receipt)
}

/// 写入反向核销分配、冲减进度并把原回款置为已冲正。
///
/// # 错误
/// 超额冲减或仓储失败时返回错误。
/// 在退货域累计额度检查成功后逆向核销并更新原回款；始终复用调用方 Executor。
/// 分配计划、额度写入、逐行 ID 与状态迁移沿原顺序执行。
pub async fn reverse_receipt_allocations(
    db: &Database,
    receipt: CustomerReceipt,
    amount: Amount,
    occurred_at: Instant,
    actor_id: &str,
    session: &mut dyn Executor,
) -> Result<()> {
    let allocations = db
        .receipt_allocations()
        .find_allocations_by_receipts(&[receipt.base.id.clone().into()], session)
        .await?;
    let (reverse_rows, chunks) = ReceiptAllocation::plan_reverse(&allocations, amount)?;
    let seqs = ReceiptAllocation::next_allocation_seq_range(&allocations, reverse_rows.len())?;
    revert_receipt_settlements(db, &chunks, actor_id, session).await?;
    persist_reverse_allocations(db, occurred_at, &receipt, &reverse_rows, &seqs, session).await?;
    let mut receipt = receipt;
    receipt.transition(CustomerReceiptStatus::Reversed)?;
    db.customer_receipts().update(&mut receipt, session).await?;
    Ok(())
}

/// 必须在冲正状态和成功审计写入后调用，重新读取全部分配后返回排序去重销售标识。
pub async fn receipt_allocation_sales_order_ids(
    db: &Database,
    receipt_id: &CustomerReceiptId,
    session: &mut dyn Executor,
) -> Result<Vec<SalesOrderId>> {
    let allocations = db
        .receipt_allocations()
        .find_allocations_by_receipts(std::slice::from_ref(receipt_id), session)
        .await?;
    Ok(sales_order_ids_for_receipt_allocations(db, &allocations, session)
        .await?
        .into_iter()
        .map(SalesOrderId::new)
        .collect())
}

/// 按冲减块回冲应收子账已核销进度。
///
/// # 错误
/// 分录缺失或超额冲减时返回错误。
async fn revert_receipt_settlements(
    db: &Database,
    chunks: &[ReceiptReverseChunk],
    actor_id: &str,
    session: &mut dyn Executor,
) -> Result<()> {
    let facts =
        load_receivable_offset_facts(db, chunks.iter().map(|chunk| chunk.increase_entry_id.clone()), session)
            .await?;
    for chunk in chunks {
        let entry = facts
            .entries
            .get(chunk.increase_entry_id.as_ref())
            .ok_or_else(|| Error::NotFound("应收分录不存在".to_string()))?;
        if !facts.accounts.contains_key(entry.receivable_account_id.as_ref()) {
            return Err(Error::NotFound("应收往来子账不存在".to_string()));
        }
        let reverted = db
            .receivable_accounts()
            .revert_settlement(&entry.receivable_account_id, &chunk.amount, actor_id, session)
            .await?;
        if !reverted {
            return Err(Error::BusinessLogicError("冲正冲减超过已核销金额".to_string()));
        }
    }
    Ok(())
}

/// 由核销分配批量读取分录与账户，收集去重销售单 ID。
///
/// # 参数
/// * `db` - 数据库
/// * `allocations` - 原回款核销分配
/// * `session` - 调用方事务执行器
///
/// # 返回
/// 返回排序去重后的销售单 ID。
///
/// # 错误
/// 缺任一分录或账户时失败关闭。
///
/// # 约束
/// 固定两次批量读取；进度刷新仍由 Service 逐单编排。
async fn sales_order_ids_for_receipt_allocations(
    db: &Database,
    allocations: &[ReceiptAllocation],
    session: &mut dyn Executor,
) -> Result<Vec<String>> {
    let facts = load_receivable_offset_facts(
        db,
        allocations.iter().map(|allocation| allocation.receivable_entry_id.clone()),
        session,
    )
    .await?;
    let mut sales_order_ids = Vec::with_capacity(allocations.len());
    for allocation in allocations {
        let entry = facts
            .entries
            .get(allocation.receivable_entry_id.as_ref())
            .ok_or_else(|| Error::NotFound("应收分录不存在".to_string()))?;
        let account = facts
            .accounts
            .get(entry.receivable_account_id.as_ref())
            .ok_or_else(|| Error::NotFound("应收往来子账不存在".to_string()))?;
        sales_order_ids.push(account.sales_order_id.to_string());
    }
    sales_order_ids.sort();
    sales_order_ids.dedup();
    Ok(sales_order_ids)
}

/// 写入反向核销分配。
///
/// # 错误
/// 仓储失败时返回错误。
async fn persist_reverse_allocations(
    db: &Database,
    occurred_at: Instant,
    receipt: &CustomerReceipt,
    reverse_rows: &[ReceiptReversePlanRow],
    seqs: &[u32],
    session: &mut dyn Executor,
) -> Result<()> {
    for (reverse, seq) in reverse_rows.iter().zip(seqs.iter()) {
        let allocation = ReceiptAllocation::new(
            ReceiptAllocationId::new(next_id()),
            ReceiptAllocationData {
                customer_receipt_id: receipt.base.id.clone().into(),
                receivable_entry_id: reverse.entry_id.clone(),
                allocation_seq: *seq,
                allocation_action: ReceivableAllocationAction::Reverse,
                allocated_amount: reverse.amount,
                allocated_at: occurred_at,
                reverses_allocation_id: Some(reverse.original_id.clone()),
            },
        )?;
        db.receipt_allocations().create(&allocation, session).await?;
    }
    Ok(())
}
