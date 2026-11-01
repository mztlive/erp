//! 回款冲正与付款冲正的原事实 `$in` 查询。

use erp_core::ids::{CustomerReceiptId, SupplierPaymentId};
use mongodb::bson::doc;
use persistence_core::{Executor, Repository, Result};

use crate::entity::returns::{PaymentReversal, ReceiptReversal};

/// 回款冲正集合仓储扩展。
#[allow(async_fn_in_trait)]
pub trait ReceiptReversalRepositoryExt {
    /// 批量按原回款集合取回冲正单（`$in`，用于累计有效冲正校验）。
    ///
    /// # 参数
    /// * `receipt_ids` - 原客户回款 ID 集合
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回全部匹配冲正单。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    async fn find_reversals_by_receipts(
        &self,
        receipt_ids: &[CustomerReceiptId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<ReceiptReversal>>;
}

impl ReceiptReversalRepositoryExt for Repository<'_, ReceiptReversal> {
    async fn find_reversals_by_receipts(
        &self,
        receipt_ids: &[CustomerReceiptId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<ReceiptReversal>> {
        if receipt_ids.is_empty() {
            return Ok(Vec::new());
        }
        let receipt_ids: Vec<String> = receipt_ids.iter().map(ToString::to_string).collect();
        self.find_many(doc! { "original_customer_receipt_id": { "$in": receipt_ids } }, executor).await
    }
}

/// 付款冲正集合仓储扩展。
#[allow(async_fn_in_trait)]
pub trait PaymentReversalRepositoryExt {
    /// 批量按原付款集合取回冲正单（`$in`，用于累计有效冲正校验）。
    ///
    /// # 参数
    /// * `payment_ids` - 原供应商付款 ID 集合
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回全部匹配冲正单。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    async fn find_reversals_by_payments(
        &self,
        payment_ids: &[SupplierPaymentId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<PaymentReversal>>;
}

impl PaymentReversalRepositoryExt for Repository<'_, PaymentReversal> {
    async fn find_reversals_by_payments(
        &self,
        payment_ids: &[SupplierPaymentId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<PaymentReversal>> {
        if payment_ids.is_empty() {
            return Ok(Vec::new());
        }
        let payment_ids: Vec<String> = payment_ids.iter().map(ToString::to_string).collect();
        self.find_many(doc! { "original_supplier_payment_id": { "$in": payment_ids } }, executor).await
    }
}
