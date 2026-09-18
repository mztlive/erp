//! 客户退款的财务核销回冲、减少分录和抵销写入；不改变原回款状态。
use async_trait::async_trait;
use erp_core::common::time::Instant;
use erp_core::ids::{CustomerReceiptId, ReceiptAllocationId, ReceivableEntryId, ReceivableEntryOffsetId};
use erp_core::money::Amount;
use id_generator::next_id;
use mongodb::Database;
use persistence_core::Executor;

use super::receipt_reversal::load_receivable_offset_facts;
use crate::entity::receivable::{
    AllocationAction as ReceivableAllocationAction, CustomerReceipt, CustomerReceiptStatus,
    EntryDirection as ReceivableEntryDirection, ReceiptAllocation, ReceiptAllocationData,
    ReceiptReverseChunk, ReceivableAccount, ReceivableEntry, ReceivableEntryData, ReceivableEntryOffset,
    ReceivableEntryOffsetData, ReceivableEntryType,
};
use crate::repository::ReceivableExt;
use crate::repository::prelude::*;
use crate::service::offset_index::OffsetFacts;
use crate::{Error, Result};
/// 客户退款实际消费的财务仓储边界；生产算法保持全部事实构造时点。
#[async_trait]
trait RefundStore: Send {
    async fn allocations(
        &mut self,
        receipt: &CustomerReceipt,
        ex: &mut dyn Executor,
    ) -> Result<Vec<ReceiptAllocation>>;
    async fn offset_facts(
        &mut self,
        chunks: &[ReceiptReverseChunk],
        ex: &mut dyn Executor,
    ) -> Result<OffsetFacts<ReceivableEntry, ReceivableAccount>>;
    async fn revert_settlement(
        &mut self,
        entry: &ReceivableEntry,
        chunk: &ReceiptReverseChunk,
        actor_id: &str,
        ex: &mut dyn Executor,
    ) -> Result<bool>;
    async fn offset(&mut self, offset: &ReceivableEntryOffset, ex: &mut dyn Executor) -> Result<()>;
    async fn decrease_entry(&mut self, entry: &ReceivableEntry, ex: &mut dyn Executor) -> Result<()>;
    async fn reverse_allocation(
        &mut self,
        allocation: &ReceiptAllocation,
        ex: &mut dyn Executor,
    ) -> Result<()>;
}
struct MongoRefundStore<'a>(&'a Database);
#[async_trait]
impl RefundStore for MongoRefundStore<'_> {
    async fn allocations(
        &mut self,
        receipt: &CustomerReceipt,
        ex: &mut dyn Executor,
    ) -> Result<Vec<ReceiptAllocation>> {
        Ok(self
            .0
            .receipt_allocations()
            .find_allocations_by_receipts(&[receipt.base.id.clone().into()], ex)
            .await?)
    }
    async fn offset_facts(
        &mut self,
        chunks: &[ReceiptReverseChunk],
        ex: &mut dyn Executor,
    ) -> Result<OffsetFacts<ReceivableEntry, ReceivableAccount>> {
        load_receivable_offset_facts(self.0, chunks.iter().map(|chunk| chunk.increase_entry_id.clone()), ex)
            .await
    }
    async fn revert_settlement(
        &mut self,
        entry: &ReceivableEntry,
        chunk: &ReceiptReverseChunk,
        actor_id: &str,
        ex: &mut dyn Executor,
    ) -> Result<bool> {
        Ok(self
            .0
            .receivable_accounts()
            .revert_settlement(&entry.receivable_account_id, &chunk.amount, actor_id, ex)
            .await?)
    }
    async fn offset(&mut self, offset: &ReceivableEntryOffset, ex: &mut dyn Executor) -> Result<()> {
        self.0.receivable_entry_offsets().create(offset, ex).await?;
        Ok(())
    }
    async fn decrease_entry(&mut self, entry: &ReceivableEntry, ex: &mut dyn Executor) -> Result<()> {
        self.0.receivable_entries().create(entry, ex).await?;
        Ok(())
    }
    async fn reverse_allocation(
        &mut self,
        allocation: &ReceiptAllocation,
        ex: &mut dyn Executor,
    ) -> Result<()> {
        self.0.receipt_allocations().create(allocation, ex).await?;
        Ok(())
    }
}
/// 财务执行客户退款所需的最小资金事实，不持有退款聚合。
pub struct CustomerRefundPosting {
    /// 退款事实身份，原来源单据/版本共用此值。
    pub refund_id: String,
    /// 本次退款总额。
    pub amount: Amount,
    /// 原退款发生时间。
    pub occurred_at: Instant,
}
/// 在调用方指定执行器中读取退款来源；预读不增加Posted守卫。
pub async fn load_customer_refund_source(
    db: &Database,
    id: &str,
    executor: &mut dyn Executor,
) -> Result<CustomerReceipt> {
    db.customer_receipts()
        .find_by_id(id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("原客户回款不存在".to_string()))
}
/// 最终退款在原存在性、Posted顺序读取来源；不复用冲正的不同错误文案。
pub async fn load_posted_refund_receipt(
    db: &Database,
    id: &CustomerReceiptId,
    executor: &mut dyn Executor,
) -> Result<CustomerReceipt> {
    let receipt = db
        .customer_receipts()
        .find_by_id(id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("原回款不存在".to_string()))?;
    if receipt.status != CustomerReceiptStatus::Posted {
        return Err(Error::BusinessLogicError("只有已过账回款可以退款".to_string()));
    }
    Ok(receipt)
}
/// 写入反向核销分配、冲减进度与减少分录。
///
/// # 错误
/// 跨主体、超额冲减或仓储失败时返回错误。
pub async fn persist_refund_offsets_and_reversals(
    db: &Database,
    refund: &CustomerRefundPosting,
    receipt: &crate::entity::receivable::CustomerReceipt,
    actor_id: &str,
    session: &mut dyn Executor,
) -> Result<()> {
    post_with_store(&mut MongoRefundStore(db), refund, receipt, actor_id, session).await
}
/// 生产仓储及替身共同执行的客户退款逆向算法。
async fn post_with_store(
    store: &mut impl RefundStore,
    refund: &CustomerRefundPosting,
    receipt: &CustomerReceipt,
    actor_id: &str,
    session: &mut dyn Executor,
) -> Result<()> {
    let allocations = store.allocations(receipt, session).await?;
    let (reverse_rows, chunks) = ReceiptAllocation::plan_reverse(&allocations, refund.amount)?;
    let seqs = ReceiptAllocation::next_allocation_seq_range(&allocations, reverse_rows.len())?;
    let decrease_entry = create_decrease_offsets(store, refund, receipt, actor_id, &chunks, session).await?;
    if let Some(entry) = decrease_entry {
        store.decrease_entry(&entry, session).await?;
    }
    for (reverse, seq) in reverse_rows.iter().zip(seqs.iter()) {
        let allocation = ReceiptAllocation::new(
            ReceiptAllocationId::new(next_id()),
            ReceiptAllocationData {
                customer_receipt_id: receipt.base.id.clone().into(),
                receivable_entry_id: reverse.entry_id.clone(),
                allocation_seq: *seq,
                allocation_action: ReceivableAllocationAction::Reverse,
                allocated_amount: reverse.amount,
                allocated_at: refund.occurred_at,
                reverses_allocation_id: Some(reverse.original_id.clone()),
            },
        )?;
        store.reverse_allocation(&allocation, session).await?;
    }
    Ok(())
}
/// 按冲减块写减少分录抵销并回冲已核销进度。
///
/// # 错误
/// 分录缺失、跨主体或超额冲减时返回错误。
async fn create_decrease_offsets(
    store: &mut impl RefundStore,
    refund: &CustomerRefundPosting,
    receipt: &crate::entity::receivable::CustomerReceipt,
    actor_id: &str,
    chunks: &[crate::entity::receivable::ReceiptReverseChunk],
    session: &mut dyn Executor,
) -> Result<Option<ReceivableEntry>> {
    let facts = store.offset_facts(chunks, session).await?;
    let mut decrease_entry: Option<ReceivableEntry> = None;
    for (offset_index, chunk) in chunks.iter().enumerate() {
        let entry = facts
            .entries
            .get(chunk.increase_entry_id.as_ref())
            .ok_or_else(|| Error::NotFound("应收分录不存在".to_string()))?;
        let account = facts
            .accounts
            .get(entry.receivable_account_id.as_ref())
            .ok_or_else(|| Error::NotFound("应收往来子账不存在".to_string()))?;
        if account.counterparty_party_id != receipt.counterparty_party_id {
            return Err(Error::BusinessLogicError("禁止跨往来主体退款".to_string()));
        }
        revert_customer_refund_settlement(store, entry, chunk, actor_id, session).await?;
        if decrease_entry.is_none() {
            decrease_entry = Some(build_customer_refund_decrease_entry(refund, entry)?);
        }
        persist_customer_refund_decrease_offset(store, decrease_entry.as_ref(), chunk, offset_index, session)
            .await?;
    }
    Ok(decrease_entry)
}
/// 原子回冲应收子账已核销进度。
///
/// # 错误
/// 超额冲减或仓储失败时返回错误。
async fn revert_customer_refund_settlement(
    store: &mut impl RefundStore,
    entry: &ReceivableEntry,
    chunk: &crate::entity::receivable::ReceiptReverseChunk,
    actor_id: &str,
    session: &mut dyn Executor,
) -> Result<()> {
    let reverted = store.revert_settlement(entry, chunk, actor_id, session).await?;
    if !reverted {
        return Err(Error::BusinessLogicError("退款冲减超过已核销金额".to_string()));
    }
    Ok(())
}
/// 构造客户退款减少应收分录。
///
/// # 错误
/// 分录字段校验失败时返回错误。
fn build_customer_refund_decrease_entry(
    refund: &CustomerRefundPosting,
    entry: &ReceivableEntry,
) -> Result<ReceivableEntry> {
    Ok(ReceivableEntry::new(
        ReceivableEntryId::new(next_id()),
        ReceivableEntryData {
            receivable_account_id: entry.receivable_account_id.clone(),
            entry_type: ReceivableEntryType::Refund,
            direction: ReceivableEntryDirection::Decrease,
            amount: refund.amount,
            due_date: erp_core::common::time::BusinessDate::today(),
            source_fact_type: "customer_refund".to_string(),
            source_document_id: refund.refund_id.clone(),
            source_revision_id: refund.refund_id.clone(),
            source_sequence: 1,
            posted_at: refund.occurred_at,
        },
    )?)
}
/// 写入一条客户退款减少分录抵销。
///
/// # 错误
/// 减少分录缺失或仓储失败时返回错误。
async fn persist_customer_refund_decrease_offset(
    store: &mut impl RefundStore,
    decrease_entry: Option<&ReceivableEntry>,
    chunk: &crate::entity::receivable::ReceiptReverseChunk,
    offset_index: usize,
    session: &mut dyn Executor,
) -> Result<()> {
    let decrease_id = decrease_entry
        .ok_or_else(|| Error::Internal("退款减少分录未创建".to_string()))?
        .base
        .id
        .clone()
        .into();
    store
        .offset(
            &ReceivableEntryOffset::new(
                ReceivableEntryOffsetId::new(next_id()),
                ReceivableEntryOffsetData {
                    decrease_entry_id: decrease_id,
                    increase_entry_id: chunk.increase_entry_id.clone(),
                    offset_sequence: offset_index as u32 + 1,
                    offset_amount: chunk.amount,
                },
            )?,
            session,
        )
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests;
