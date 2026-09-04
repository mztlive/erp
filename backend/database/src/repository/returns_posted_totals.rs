//! 域 D21 `returns` 已过账累计聚合：退款与冲正过账额度只统计 `posted`。
//!
//! 草稿与审批中不占正式额度，已冲正单据不再计入；四个聚合均使用 Decimal128
//! `$sum`，不得加载完整实体后在内存折叠。

use entities::ids::{CustomerReceiptId, SupplierPaymentId};
use entities::money::Amount;
use entities::returns::{CustomerRefund, PaymentReversal, ReceiptReversal, SupplierRefund};
use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use futures_util::TryStreamExt;
use mongodb::bson::{doc, Document};
use serde::Deserialize;

use super::Repository;
use crate::executor::Executor;
use crate::Result;

/// 已过账金额聚合行（Decimal128 求和结果）。
#[derive(Debug, Deserialize)]
struct PostedAmountTotalRow {
    /// 已过账合计；聚合保证存在。
    total: Amount,
}

/// 在调用方执行器上运行单值金额聚合管道。
///
/// # 参数
/// * `collection` - 目标集合句柄
/// * `pipeline` - `$match` + `$group` + `$project` 管道
/// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
///
/// # 返回
/// 无匹配时返回精确零，否则返回聚合合计。
///
/// # 错误
/// MongoDB 聚合、游标读取或 Decimal128 反序列化失败时返回错误。
///
/// # 关键业务约束
/// 事务视图与写入共享同一执行器；失败时不得产生部分提交。
async fn posted_total<T>(
    collection: &mongodb::Collection<T>,
    pipeline: Vec<Document>,
    executor: &mut dyn Executor,
) -> Result<Amount>
where
    T: Send + Sync,
{
    let rows = match executor.session() {
        Some(session) => {
            collection
                .aggregate(pipeline)
                .with_type::<PostedAmountTotalRow>()
                .session(&mut *session)
                .await?
                .stream(session)
                .try_collect::<Vec<_>>()
                .await?
        }
        None => {
            collection
                .aggregate(pipeline)
                .with_type::<PostedAmountTotalRow>()
                .await?
                .try_collect::<Vec<_>>()
                .await?
        }
    };

    Ok(rows.into_iter().next().map_or_else(Amount::zero, |row| row.total))
}

/// 构造已过账金额聚合管道。
///
/// # 参数
/// * `original_field` - 原单外键字段名
/// * `original_id` - 原单 ID 字符串
/// * `exclude_id` - 本次过账单据 ID，聚合中排除
///
/// # 返回
/// 返回 `$match` + `$group` + `$project` 三段管道。
///
/// # 错误
/// 无。
///
/// # 关键业务约束
/// `$match` 只统计 `posted`，限定原单等值、排除当前单并排除软删除；
/// 前缀命中既有原单追溯索引，不得新增索引或迁移。
fn posted_total_pipeline(original_field: &str, original_id: &str, exclude_id: &str) -> Vec<Document> {
    vec![
        doc! {
            "$match": {
                original_field: original_id,
                "status": "posted",
                "id": { "$ne": exclude_id },
                "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
            }
        },
        doc! {
            "$group": {
                "_id": mongodb::bson::Bson::Null,
                "total": { "$sum": "$amount" },
            }
        },
        doc! { "$project": { "_id": 0 } },
    ]
}

impl<'a> Repository<'a, CustomerRefund> {
    /// 按原回款聚合已过账客户退款合计。
    ///
    /// # 参数
    /// * `receipt_id` - 原客户回款
    /// * `exclude_id` - 本次过账退款 ID，聚合中排除
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回已过账合计；无匹配时返回精确零。
    ///
    /// # 错误
    /// MongoDB 聚合、游标读取或 Decimal128 反序列化失败时返回错误。
    ///
    /// # 关键业务约束
    /// 只统计 `posted`，草稿、审批中与已冲正不占额度；使用 Decimal128 `$sum`；
    /// 复用 `idx_customer_refunds_original` 前缀，无新增索引与迁移。
    pub async fn posted_refund_total_by_receipt(
        &self,
        receipt_id: &CustomerReceiptId,
        exclude_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Amount> {
        let pipeline = posted_total_pipeline("original_receipt_id", receipt_id.as_ref(), exclude_id);

        posted_total(&self.collection(), pipeline, executor).await
    }
}

impl<'a> Repository<'a, SupplierRefund> {
    /// 按原付款聚合已过账供应商退款合计。
    ///
    /// # 参数
    /// * `payment_id` - 原供应商付款
    /// * `exclude_id` - 本次过账退款 ID，聚合中排除
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回已过账合计；无匹配时返回精确零。
    ///
    /// # 错误
    /// MongoDB 聚合、游标读取或 Decimal128 反序列化失败时返回错误。
    ///
    /// # 关键业务约束
    /// 只统计 `posted`，草稿、审批中与已冲正不占额度；使用 Decimal128 `$sum`；
    /// 复用 `idx_supplier_refunds_original` 前缀，无新增索引与迁移。
    pub async fn posted_refund_total_by_payment(
        &self,
        payment_id: &SupplierPaymentId,
        exclude_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Amount> {
        let pipeline = posted_total_pipeline("original_payment_id", payment_id.as_ref(), exclude_id);

        posted_total(&self.collection(), pipeline, executor).await
    }
}

impl<'a> Repository<'a, ReceiptReversal> {
    /// 按原回款聚合已过账回款冲正合计。
    ///
    /// # 参数
    /// * `receipt_id` - 原客户回款
    /// * `exclude_id` - 本次过账冲正 ID，聚合中排除
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回已过账合计；无匹配时返回精确零。
    ///
    /// # 错误
    /// MongoDB 聚合、游标读取或 Decimal128 反序列化失败时返回错误。
    ///
    /// # 关键业务约束
    /// 只统计 `posted`，草稿、审批中与已冲正不占额度；使用 Decimal128 `$sum`；
    /// 命中 `idx_receipt_reversals_original`，无新增索引与迁移。
    pub async fn posted_reversal_total_by_receipt(
        &self,
        receipt_id: &CustomerReceiptId,
        exclude_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Amount> {
        let pipeline = posted_total_pipeline("original_customer_receipt_id", receipt_id.as_ref(), exclude_id);

        posted_total(&self.collection(), pipeline, executor).await
    }
}

impl<'a> Repository<'a, PaymentReversal> {
    /// 按原付款聚合已过账付款冲正合计。
    ///
    /// # 参数
    /// * `payment_id` - 原供应商付款
    /// * `exclude_id` - 本次过账冲正 ID，聚合中排除
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回已过账合计；无匹配时返回精确零。
    ///
    /// # 错误
    /// MongoDB 聚合、游标读取或 Decimal128 反序列化失败时返回错误。
    ///
    /// # 关键业务约束
    /// 只统计 `posted`，草稿、审批中与已冲正不占额度；使用 Decimal128 `$sum`；
    /// 命中 `idx_payment_reversals_original`，无新增索引与迁移。
    pub async fn posted_reversal_total_by_payment(
        &self,
        payment_id: &SupplierPaymentId,
        exclude_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Amount> {
        let pipeline = posted_total_pipeline("original_supplier_payment_id", payment_id.as_ref(), exclude_id);

        posted_total(&self.collection(), pipeline, executor).await
    }
}

#[cfg(test)]
mod tests {
    use super::posted_total_pipeline;

    /// 管道限定原单、已过账、排除当前单与未删除。
    #[test]
    fn pipeline_matches_original_and_posted_only() {
        let pipeline = posted_total_pipeline("original_receipt_id", "rcpt-1", "refund-9");
        let matched = pipeline[0].get_document("$match").expect("过滤阶段");

        assert_eq!(matched.get_str("original_receipt_id").unwrap(), "rcpt-1");
        assert_eq!(matched.get_str("status").unwrap(), "posted");
        assert_eq!(
            matched.get_document("id").unwrap().get_str("$ne").unwrap(),
            "refund-9"
        );
        assert_eq!(matched.get_i64("deleted_at").expect("未删除条件"), 0);

        let group = pipeline[1].get_document("$group").expect("分组阶段");

        assert_eq!(
            group.get_document("total").unwrap().get_str("$sum").unwrap(),
            "$amount"
        );
    }

    /// 四类原单字段名不同但管道形态一致。
    #[test]
    fn pipeline_covers_all_original_fields() {
        for field in [
            "original_receipt_id",
            "original_payment_id",
            "original_customer_receipt_id",
            "original_supplier_payment_id",
        ] {
            let pipeline = posted_total_pipeline(field, "orig-1", "cur-1");
            let matched = pipeline[0].get_document("$match").expect("过滤阶段");

            assert_eq!(matched.get_str(field).unwrap(), "orig-1");
            assert_eq!(matched.get_str("status").unwrap(), "posted");
        }
    }
}
