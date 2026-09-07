use crate::entity::payable::{PayableAccount, PayableEntry, PaymentAllocation, PurchaseInvoiceAllocation};

use super::super::extensions::PayableExt;
use super::{PayableRepository, PAYABLE_ENTRIES};
use persistence_core::Executor;
use persistence_core::{mongo_ops, Result};

impl<'a> PayableRepository<'a> {
    /// 建立应付往来子账与原始应付分录（跨集合多步骤写入）。
    ///
    /// 依次写入 `payable_accounts` 与 `payable_entries`，保证「子账 + 原始
    /// 应付」原子可见（数据模型 §6.9 采购单财务审核通过或供应商结算单确认后
    /// 才形成原始应付）。
    /// **必须收到事务执行器**：本方法不构成原子边界，传入 `NoTransaction` 时
    /// 两笔写入各自自动提交，第二笔失败会留下只有子账没有分录的半成品；
    /// Service 必须通过 `persistence_core::Transactional::with_transaction` 传入事务会话。
    ///
    /// # 参数
    /// * `account` - 待写入的应付往来子账
    /// * `entry` - 待写入的原始应付分录
    /// * `executor` - 数据访问执行器，必须位于事务中
    ///
    /// # 错误
    /// 当唯一索引冲突（透出 [`persistence_core::Error::DuplicateKey`]，由 Service 映射
    /// 为冲突语义）或 MongoDB 写入失败时返回错误。
    pub async fn create_payable_with_entry(
        &self,
        account: &PayableAccount,
        entry: &PayableEntry,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        mongo_ops::insert_one(
            &self
                .db
                .collection::<PayableAccount>(<mongodb::Database as PayableExt>::PAYABLE_ACCOUNTS),
            account,
            executor,
        )
        .await?;
        mongo_ops::insert_one(
            &self.db.collection::<PayableEntry>(PAYABLE_ENTRIES),
            entry,
            executor,
        )
        .await?;
        Ok(())
    }

    /// 批量写入付款核销分配（`insert_many`，禁止逐笔插入）。
    ///
    /// 一次批量写入同一付款单的核销分配；空输入不访问数据库。分配行是
    /// 正式事实，`(supplier_payment_id, allocation_seq)` 唯一索引在并发重复
    /// 过账时抛出唯一键冲突，由 Service 转译并整体回滚。
    /// **必须收到事务执行器**：本方法不构成原子边界，Service 必须通过
    /// `persistence_core::Transactional::with_transaction` 传入事务会话。
    ///
    /// # 参数
    /// * `allocations` - 待持久化的核销分配
    /// * `executor` - 数据访问执行器，必须位于事务中
    ///
    /// # 返回
    /// 全部写入成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 当唯一索引冲突（透出 [`persistence_core::Error::DuplicateKey`]，由 Service 映射
    /// 为冲突语义）或 MongoDB 写入失败时返回错误。
    pub async fn create_payment_allocations_many(
        &self,
        allocations: &[PaymentAllocation],
        executor: &mut dyn Executor,
    ) -> Result<()> {
        mongo_ops::insert_many(
            &self
                .db
                .collection::<PaymentAllocation>(<mongodb::Database as PayableExt>::PAYMENT_ALLOCATIONS),
            allocations.to_vec(),
            executor,
        )
        .await
    }

    /// 批量写入进项发票分配（`insert_many`，禁止逐笔插入）。
    ///
    /// 一次批量写入同一进项发票的分配；空输入不访问数据库。分配行是
    /// 正式事实，`(invoice_id, allocation_seq)` 唯一索引在并发重复登记时抛出
    /// 唯一键冲突，由 Service 转译并整体回滚。
    /// **必须收到事务执行器**：本方法不构成原子边界，Service 必须通过
    /// `persistence_core::Transactional::with_transaction` 传入事务会话。
    ///
    /// # 参数
    /// * `allocations` - 待持久化的进项发票分配
    /// * `executor` - 数据访问执行器，必须位于事务中
    ///
    /// # 返回
    /// 全部写入成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 当唯一索引冲突（透出 [`persistence_core::Error::DuplicateKey`]，由 Service 映射
    /// 为冲突语义）或 MongoDB 写入失败时返回错误。
    pub async fn create_purchase_invoice_allocations_many(
        &self,
        allocations: &[PurchaseInvoiceAllocation],
        executor: &mut dyn Executor,
    ) -> Result<()> {
        mongo_ops::insert_many(
            &self.db.collection::<PurchaseInvoiceAllocation>(
                <mongodb::Database as PayableExt>::PURCHASE_INVOICE_ALLOCATIONS,
            ),
            allocations.to_vec(),
            executor,
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::PayableRepository;
    use persistence_core::NoTransaction;

    /// 空输入必须直接成功且不访问数据库。
    #[tokio::test]
    async fn create_purchase_invoice_allocations_many_empty_input_without_db() {
        let client = mongodb::Client::with_uri_str("mongodb://127.0.0.1:1")
            .await
            .expect("客户端句柄创建失败");
        let database = client.database("unused");
        let repository = PayableRepository::new(&database);
        repository
            .create_purchase_invoice_allocations_many(&[], &mut NoTransaction)
            .await
            .expect("空输入批量插入必须成功");
    }
}
