//! 按应收项目汇总有效审批占用；复用 pending_entry 索引。

use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use erp_core::ids::ReceivableEntryId;
use futures_util::TryStreamExt;
use mongodb::bson::doc;
use persistence_core::{Executor, Repository, Result};

use crate::entity::receivable::{CustomerReceipt, CustomerReceiptStatus, PendingReceiptAllocation};

#[allow(async_fn_in_trait)]
pub trait ReceiptCapacityRepositoryExt {
    /// 汇总指定分录的审批中拟核销金额。
    ///
    /// # 参数
    /// * `entry_ids` - 已限定子账的全部分录，空集合不查询。
    /// * `excluded_receipt` - 最终核销或重提时排除本单；读取候选传 `None`。
    /// * `executor` - 调用方事务。
    ///
    /// # 返回
    /// 每个分录最多一行金额；草稿、已核销和已冲正单据不占用。
    ///
    /// # 错误
    /// 数据读取或金额反序列化失败时返回错误。
    async fn reserved_allocations(
        &self,
        entry_ids: &[ReceivableEntryId],
        excluded_receipt: Option<&str>,
        executor: &mut dyn Executor,
    ) -> Result<Vec<PendingReceiptAllocation>>;
}

impl ReceiptCapacityRepositoryExt for Repository<'_, CustomerReceipt> {
    async fn reserved_allocations(
        &self,
        entry_ids: &[ReceivableEntryId],
        excluded_receipt: Option<&str>,
        executor: &mut dyn Executor,
    ) -> Result<Vec<PendingReceiptAllocation>> {
        if entry_ids.is_empty() {
            return Ok(Vec::new());
        }
        let ids = entry_ids.iter().map(ToString::to_string).collect::<Vec<_>>();
        let mut filter = doc! {
            "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
            "status": CustomerReceiptStatus::InApproval.as_str(),
            "pending_allocations.receivable_entry_id": { "$in": &ids },
        };
        if let Some(id) = excluded_receipt {
            filter.insert("id", doc! { "$ne": id });
        }
        let pipeline = vec![
            doc! { "$match": filter },
            doc! { "$unwind": "$pending_allocations" },
            doc! { "$match": { "pending_allocations.receivable_entry_id": { "$in": ids } } },
            doc! { "$group": { "_id": "$pending_allocations.receivable_entry_id",
            "allocated_amount": { "$sum": "$pending_allocations.allocated_amount" } } },
            doc! { "$project": { "_id": 0, "receivable_entry_id": "$_id", "allocated_amount": 1 } },
        ];
        let collection = self.collection();
        Ok(match executor.session() {
            Some(session) => {
                collection
                    .aggregate(pipeline)
                    .with_type::<PendingReceiptAllocation>()
                    .session(&mut *session)
                    .await?
                    .stream(session)
                    .try_collect()
                    .await?
            },
            None => {
                collection
                    .aggregate(pipeline)
                    .with_type::<PendingReceiptAllocation>()
                    .await?
                    .try_collect()
                    .await?
            },
        })
    }
}
