//! 自动核销只读取原采购应付的有界事实，不接受客户端分录选择。

use mongodb::Database;
use mongodb::bson::doc;
use mongodb::options::FindOptions;
use persistence_core::{Executor, mongo_ops};

use crate::entity::payable::{PayableEntry, PayableEntryOffset, PaymentAllocation};
use crate::repository::PayableExt;
use crate::{Error, Result};

/// 读取原采购应付的完整增加分录，不截断后继续核销。
/// 返回最多一千条稳定排序的增加分录。
/// 候选超过上限或数据库失败时拒绝。
///
/// 输入约定：
/// - `db` / `payable_account` / `executor`：财务数据、原应付和核销事务。
pub async fn automatic_settlement_entries(
    db: &Database,
    payable_account: &str,
    executor: &mut dyn Executor,
) -> Result<Vec<PayableEntry>> {
    let entries: Vec<PayableEntry> = mongo_ops::find_many(
        &db.payable_entries().collection(),
        doc! {"payable_account_id": payable_account, "direction": "increase", "deleted_at": 0_i64},
        FindOptions::builder().sort(doc! {"posted_at": 1, "id": 1}).limit(1001).build(),
        executor,
    )
    .await?;
    if entries.len() > 1000 {
        return Err(Error::BusinessLogicError("自动核销增加分录超过上限，请核对原采购应付".into()));
    }
    Ok(entries)
}

/// 读取候选分录全部来源核销，防止忽略普通付款或其他使用而重复分配。
/// 返回最多一万条正反向核销；空分录不查询。
/// 失败条件：超限或数据库失败时拒绝，不使用部分结果。
///
/// 输入约定：
/// - `db` / `entries` / `executor`：数据源、完整分录和当前事务。
pub async fn automatic_settlement_allocations(
    db: &Database,
    entries: &[PayableEntry],
    executor: &mut dyn Executor,
) -> Result<Vec<PaymentAllocation>> {
    if entries.is_empty() {
        return Ok(Vec::new());
    }
    let ids = entries.iter().map(|entry| entry.base.id.as_str()).collect::<Vec<_>>();
    let rows: Vec<PaymentAllocation> = mongo_ops::find_many(
        &db.payment_allocations().collection(),
        doc! {"payable_entry_id": {"$in": ids}, "deleted_at": 0_i64},
        FindOptions::builder().limit(10001).build(),
        executor,
    )
    .await?;
    if rows.len() > 10000 {
        return Err(Error::BusinessLogicError("自动核销历史分配超过上限，请核对原采购应付".into()));
    }
    Ok(rows)
}

/// 读取候选增加分录的全部正式抵销，防止将已冲减应付再次核销。
/// 返回最多一万条抵销；空分录不查询。
/// 超限或数据库失败时拒绝。
///
/// 输入约定：
/// - `db` / `entries` / `executor`：原数据、完整分录及当前事务。
pub async fn automatic_settlement_offsets(
    db: &Database,
    entries: &[PayableEntry],
    executor: &mut dyn Executor,
) -> Result<Vec<PayableEntryOffset>> {
    if entries.is_empty() {
        return Ok(Vec::new());
    }
    let ids = entries.iter().map(|entry| entry.base.id.as_str()).collect::<Vec<_>>();
    let rows: Vec<PayableEntryOffset> = mongo_ops::find_many(
        &db.payable_entry_offsets().collection(),
        doc! {"increase_entry_id": {"$in": ids}, "deleted_at": 0_i64},
        FindOptions::builder().limit(10001).build(),
        executor,
    )
    .await?;
    if rows.len() > 10000 {
        return Err(Error::BusinessLogicError("自动核销历史抵销超过上限，请核对原采购应付".into()));
    }
    Ok(rows)
}
