//! 供应商退款原事实 `$in` 查询。

use erp_core::ids::{PayableEntryId, SupplierPaymentId};
use mongodb::bson::Document;
use persistence_core::{Executor, Repository, Result};

use super::search::originals_filter;
use crate::entity::returns::SupplierRefund;

/// 供应商退款集合仓储扩展。
#[allow(async_fn_in_trait)]
pub trait SupplierRefundRepositoryExt {
    /// 批量按原事实取回供应商退款（`$in`，用于累计冲正校验）。
    ///
    /// # 参数
    /// * `payment_ids` - 原付款 ID 集合（可为空）
    /// * `entry_ids` - 原应付分录 ID 集合（可为空）
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回全部匹配退款。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    async fn find_refunds_by_originals(
        &self,
        payment_ids: &[SupplierPaymentId],
        entry_ids: &[PayableEntryId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<SupplierRefund>>;
}

impl SupplierRefundRepositoryExt for Repository<'_, SupplierRefund> {
    async fn find_refunds_by_originals(
        &self,
        payment_ids: &[SupplierPaymentId],
        entry_ids: &[PayableEntryId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<SupplierRefund>> {
        let filter = supplier_refund_originals_filter(payment_ids, entry_ids);
        self.find_many(filter, executor).await
    }
}

/// 保留原事实筛选：两个来源集合都非空时同时匹配，都为空时交给基础仓储查询全部未删除事实。
fn supplier_refund_originals_filter(
    payment_ids: &[SupplierPaymentId],
    entry_ids: &[PayableEntryId],
) -> Document {
    originals_filter(payment_ids, "original_payment_id", entry_ids, "original_payable_entry_id")
}

#[cfg(test)]
mod original_lookup_contract {
    use erp_core::ids::{PayableEntryId, SupplierPaymentId};
    use mongodb::bson::{Document, doc};

    use super::supplier_refund_originals_filter;

    #[test]
    fn supplier_refund_sources_keep_and_and_empty_lookup() {
        let payments = [SupplierPaymentId::new("payment-1")];
        let entries = [PayableEntryId::new("entry-1")];
        assert_eq!(supplier_refund_originals_filter(&[], &[]), Document::new());
        assert_eq!(
            supplier_refund_originals_filter(&payments, &[]),
            doc! {
                "original_payment_id": { "$in": ["payment-1"] },
            }
        );
        assert_eq!(
            supplier_refund_originals_filter(&[], &entries),
            doc! {
                "original_payable_entry_id": { "$in": ["entry-1"] },
            }
        );
        assert_eq!(
            supplier_refund_originals_filter(&payments, &entries),
            doc! {
                "original_payment_id": { "$in": ["payment-1"] },
                "original_payable_entry_id": { "$in": ["entry-1"] },
            }
        );
    }
}
