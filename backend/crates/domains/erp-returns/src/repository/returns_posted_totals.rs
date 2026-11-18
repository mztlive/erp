//! 域 D21 `returns` 已过账累计聚合：退款与冲正过账额度只统计 `posted`。
//!
//! 草稿与审批中不占正式额度，已冲正单据不再计入；四个聚合均使用 Decimal128
//! `$sum`，不得加载完整实体后在内存折叠。

use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use erp_core::ids::{CustomerReceiptId, SupplierPaymentId};
use erp_core::money::Amount;
use mongodb::bson::{Document, doc};
use persistence_core::{Executor, Result, aggregate};
use serde::Deserialize;

use crate::entity::returns::{CustomerRefund, PaymentReversal, ReceiptReversal, SupplierRefund};

/// 已过账金额聚合行（Decimal128 求和结果）。
#[derive(Debug, Deserialize)]
struct PostedAmountTotalRow {
    /// 已过账合计；聚合保证存在。
    total: Amount,
}

async fn posted_total<T>(
    collection: &mongodb::Collection<T>,
    filter: Document,
    executor: &mut dyn Executor,
) -> Result<Amount>
where
    T: Send + Sync,
{
    let rows = aggregate::rows(
        &collection.clone_with_type::<PostedAmountTotalRow>(),
        posted_total_pipeline(filter),
        executor,
    )
    .await?;
    Ok(rows.into_iter().next().map_or_else(Amount::zero, |row| row.total))
}

fn posted_total_pipeline(filter: Document) -> Vec<Document> {
    vec![
        doc! { "$match": filter },
        doc! { "$group": { "_id": null, "total": { "$sum": "$amount" } } },
        doc! { "$project": { "_id": 0 } },
    ]
}

fn posted_total_filter(original_field: &str, original_id: &str, exclude_id: &str) -> Document {
    doc! {
        original_field: original_id,
        "status": "posted",
        "id": { "$ne": exclude_id },
        "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
    }
}

fn supplier_refund_total_filter(payment_id: &str, exclude_id: &str) -> Document {
    let mut filter = posted_total_filter("original_payment_id", payment_id, exclude_id);
    filter.insert("settlement_method", doc! { "$ne": "SUPPLIER_CREDIT" });
    filter
}

/// 客户退款已过账累计聚合。
#[allow(async_fn_in_trait)]
pub trait CustomerRefundPostedTotalsExt {
    /// 按原回款聚合已过账客户退款合计。
    /// 返回已过账合计；无匹配时返回精确零。
    /// `receipt_id` 为原客户回款；`exclude_id` 为本次过账退款 ID，聚合中排除。
    /// 沿用调用方执行器 `executor`。
    /// MongoDB 聚合、游标读取或 Decimal128 反序列化失败时返回错误。
    async fn posted_refund_total_by_receipt(
        &self,
        receipt_id: &CustomerReceiptId,
        exclude_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Amount>;
}

impl CustomerRefundPostedTotalsExt for persistence_core::Repository<'_, CustomerRefund> {
    async fn posted_refund_total_by_receipt(
        &self,
        receipt_id: &CustomerReceiptId,
        exclude_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Amount> {
        posted_total(
            &self.collection(),
            posted_total_filter("original_receipt_id", receipt_id.as_ref(), exclude_id),
            executor,
        )
        .await
    }
}

/// 供应商退款已过账累计聚合。
#[allow(async_fn_in_trait)]
pub trait SupplierRefundPostedTotalsExt {
    /// 按原付款聚合已过账供应商退款合计。
    /// 返回已过账合计；无匹配时返回精确零。
    /// `payment_id` 为原供应商付款；`exclude_id` 为本次过账退款 ID，聚合中排除。
    /// 沿用调用方执行器 `executor`。
    /// MongoDB 聚合、游标读取或 Decimal128 反序列化失败时返回错误。
    async fn posted_refund_total_by_payment(
        &self,
        payment_id: &SupplierPaymentId,
        exclude_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Amount>;
}

impl SupplierRefundPostedTotalsExt for persistence_core::Repository<'_, SupplierRefund> {
    async fn posted_refund_total_by_payment(
        &self,
        payment_id: &SupplierPaymentId,
        exclude_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Amount> {
        posted_total(
            &self.collection(),
            supplier_refund_total_filter(payment_id.as_ref(), exclude_id),
            executor,
        )
        .await
    }
}

/// 回款冲正已过账累计聚合。
#[allow(async_fn_in_trait)]
pub trait ReceiptReversalPostedTotalsExt {
    /// 按原回款聚合已过账回款冲正合计。
    /// 返回已过账合计；无匹配时返回精确零。
    /// `receipt_id` 为原客户回款；`exclude_id` 为本次过账冲正 ID，聚合中排除。
    /// 沿用调用方执行器 `executor`。
    /// MongoDB 聚合、游标读取或 Decimal128 反序列化失败时返回错误。
    async fn posted_reversal_total_by_receipt(
        &self,
        receipt_id: &CustomerReceiptId,
        exclude_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Amount>;
}

impl ReceiptReversalPostedTotalsExt for persistence_core::Repository<'_, ReceiptReversal> {
    async fn posted_reversal_total_by_receipt(
        &self,
        receipt_id: &CustomerReceiptId,
        exclude_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Amount> {
        posted_total(
            &self.collection(),
            posted_total_filter("original_customer_receipt_id", receipt_id.as_ref(), exclude_id),
            executor,
        )
        .await
    }
}

/// 付款冲正已过账累计聚合。
#[allow(async_fn_in_trait)]
pub trait PaymentReversalPostedTotalsExt {
    /// 按原付款聚合已过账付款冲正合计。
    /// 返回已过账合计；无匹配时返回精确零。
    /// `payment_id` 为原供应商付款；`exclude_id` 为本次过账冲正 ID，聚合中排除。
    /// 沿用调用方执行器 `executor`。
    /// MongoDB 聚合、游标读取或 Decimal128 反序列化失败时返回错误。
    async fn posted_reversal_total_by_payment(
        &self,
        payment_id: &SupplierPaymentId,
        exclude_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Amount>;
}

impl PaymentReversalPostedTotalsExt for persistence_core::Repository<'_, PaymentReversal> {
    async fn posted_reversal_total_by_payment(
        &self,
        payment_id: &SupplierPaymentId,
        exclude_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Amount> {
        posted_total(
            &self.collection(),
            posted_total_filter("original_supplier_payment_id", payment_id.as_ref(), exclude_id),
            executor,
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::{posted_total_filter, posted_total_pipeline, supplier_refund_total_filter};

    // 验证管道限定原单、已过账、排除当前单与未删除。
    #[test]
    fn pipeline_matches_original_and_posted_only() {
        let pipeline =
            posted_total_pipeline(posted_total_filter("original_receipt_id", "rcpt-1", "refund-9"));
        let matched = pipeline[0].get_document("$match").expect("过滤阶段");
        assert_eq!(matched.get_str("original_receipt_id").unwrap(), "rcpt-1");
        assert_eq!(matched.get_str("status").unwrap(), "posted");
        assert_eq!(matched.get_document("id").unwrap().get_str("$ne").unwrap(), "refund-9");
        assert_eq!(matched.get_i64("deleted_at").expect("未删除条件"), 0);

        let group = pipeline[1].get_document("$group").expect("分组阶段");
        assert_eq!(group.get_document("total").unwrap().get_str("$sum").unwrap(), "$amount");
    }

    #[test]
    fn pipeline_covers_all_original_fields() {
        for field in [
            "original_receipt_id",
            "original_payment_id",
            "original_customer_receipt_id",
            "original_supplier_payment_id",
        ] {
            let matched = posted_total_filter(field, "orig-1", "cur-1");
            assert_eq!(matched.get_str(field).unwrap(), "orig-1");
            assert_eq!(matched.get_str("status").unwrap(), "posted");
        }
    }

    #[test]
    fn supplier_refunds_exclude_supplier_credit_from_the_posted_cash_total() {
        let filter = supplier_refund_total_filter("payment-1", "refund-1");
        assert_eq!(filter.get_str("original_payment_id").unwrap(), "payment-1");
        assert_eq!(
            filter.get_document("settlement_method").unwrap().get_str("$ne").unwrap(),
            "SUPPLIER_CREDIT"
        );

        assert!(
            !posted_total_filter("original_receipt_id", "receipt-1", "refund-1")
                .contains_key("settlement_method")
        );
    }

    // 验证MongoDB $sum 结果必须按 Decimal128 反序列化，不经过浮点或整数金额。
    #[test]
    fn posted_amount_row_reads_exact_decimal128_sum() {
        use std::str::FromStr;

        let total = mongodb::bson::Decimal128::from_str("1234567890.12").unwrap();
        let row: super::PostedAmountTotalRow =
            mongodb::bson::deserialize_from_document(mongodb::bson::doc! { "total": total }).unwrap();
        assert_eq!(row.total, erp_core::money::Amount::from_str("1234567890.12").unwrap());
    }
}
