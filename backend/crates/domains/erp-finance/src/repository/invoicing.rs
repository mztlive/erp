//! 应收应付共用的开票进度守卫与有序条件写入。

use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use erp_core::money::Amount;
use mongodb::bson::{Bson, Document, doc};
use persistence_core::{Executor, Result};

use super::progress::{AccountProgressWrite, amount_bson, progress_pipeline};

/// 开票进度的增加或红冲方向。
#[derive(Clone, Copy)]
pub(super) enum InvoicingDirection {
    Apply,
    Revert,
}

impl InvoicingDirection {
    fn filter(self, id: &str, amount: &Bson) -> Document {
        match self {
            Self::Apply => invoicing_guard(id, amount),
            Self::Revert => doc! {
                "id": id,
                "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
                "$expr": { "$gte": ["$invoiced_total", amount] },
            },
        }
    }
}

/// 按输入顺序分别保留条件命中与未命中的强类型账户身份。
pub(super) struct InvoicingBatch<I> {
    pub applied: Vec<I>,
    pub rejected: Vec<I>,
}

/// 生成开票及收票的共同额度守卫，禁止超出可开票总额。
///
/// # 参数
/// * `id` - 应收或应付账户身份。
/// * `amount` - 已转换为 Decimal128 的含税金额。
///
/// # 返回
/// 返回只匹配未删除账户且增加后不超额度的条件。
///
/// # 错误
/// 不返回错误。
pub(super) fn invoicing_guard(id: &str, amount: &Bson) -> Document {
    doc! {
        "id": id,
        "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
        "$expr": {
            "$lte": [
                { "$add": ["$invoiced_total", amount] },
                "$invoiceable_total",
            ],
        },
    }
}

/// 执行一次开票进度条件更新，沿用账户写入与金额表示机制。
///
/// # 参数
/// * `writer` - 财务账户条件写入入口。
/// * `id` - 强类型账户身份对应的字符串。
/// * `amount` - 不做舍入或换算的含税金额。
/// * `updated_by` - 更新执行人。
/// * `direction` - 增加开票进度或红冲。
/// * `executor` - 调用方执行器。
///
/// # 返回
/// 额度条件命中时返回 `true`，不满足时返回 `false`。
///
/// # 错误
/// 金额转换或数据库更新失败时立即返回原仓储错误。
pub(super) async fn update(
    writer: &(impl AccountProgressWrite + ?Sized),
    id: &str,
    amount: &Amount,
    updated_by: &str,
    direction: InvoicingDirection,
    executor: &mut dyn Executor,
) -> Result<bool> {
    let amount = amount_bson(amount)?;
    writer
        .conditional_update(
            direction.filter(id, &amount),
            progress_pipeline(
                "invoiced_total",
                "open_invoiceable_total",
                &amount,
                matches!(direction, InvoicingDirection::Apply),
                updated_by,
            ),
            executor,
        )
        .await
}

/// 按调用方计划顺序执行批量开票或红冲，任一错误立即停止后续写入。
///
/// # 参数
/// * `writer` - 应收或应付账户共用的条件写入入口。
/// * `deltas` - 调用方已聚合的账户金额增量；本方法不重排或合并。
/// * `updated_by` - 更新执行人。
/// * `direction` - 增加开票进度或红冲。
/// * `executor` - 全部写入共用的调用方执行器。
///
/// # 返回
/// 返回分别按输入顺序保留命中及拒绝账户的结果；空输入不执行写入。
///
/// # 错误
/// 首次金额转换或数据库更新失败时返回该错误；事务回滚由调用方负责。
pub(super) async fn update_many<I>(
    writer: &(impl AccountProgressWrite + ?Sized),
    deltas: &[(I, Amount)],
    updated_by: &str,
    direction: InvoicingDirection,
    executor: &mut dyn Executor,
) -> Result<InvoicingBatch<I>>
where
    I: AsRef<str> + Clone + Sync,
{
    let mut applied = Vec::new();
    let mut rejected = Vec::new();
    for (id, amount) in deltas {
        if update(writer, id.as_ref(), amount, updated_by, direction, executor).await? {
            applied.push(id.clone());
        } else {
            rejected.push(id.clone());
        }
    }
    Ok(InvoicingBatch { applied, rejected })
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::sync::Mutex;

    use erp_core::ids::{PayableAccountId, ReceivableAccountId};
    use persistence_core::{Error, NoTransaction};

    use super::*;

    struct RecordedWrite {
        filter: Document,
        pipeline: Vec<Document>,
        executor: usize,
    }

    struct RecordingWriter {
        outcomes: Mutex<VecDeque<Result<bool>>>,
        calls: Mutex<Vec<RecordedWrite>>,
    }

    impl RecordingWriter {
        fn new(outcomes: impl IntoIterator<Item = Result<bool>>) -> Self {
            Self { outcomes: Mutex::new(outcomes.into_iter().collect()), calls: Mutex::new(Vec::new()) }
        }
    }

    impl AccountProgressWrite for RecordingWriter {
        async fn conditional_update(
            &self,
            filter: Document,
            pipeline: Vec<Document>,
            executor: &mut dyn Executor,
        ) -> Result<bool> {
            self.calls.lock().unwrap().push(RecordedWrite {
                filter,
                pipeline,
                executor: executor as *mut dyn Executor as *mut () as usize,
            });
            self.outcomes.lock().unwrap().pop_front().expect("写入次数不得超过预期")
        }
    }

    fn amount(value: &str) -> Amount {
        value.parse().unwrap()
    }

    #[tokio::test]
    async fn batch_preserves_input_order_results_amount_and_executor() {
        let writer = RecordingWriter::new([Ok(true), Ok(false), Ok(true)]);
        let deltas = vec![
            (PayableAccountId::new("p-2"), amount("12.34")),
            (PayableAccountId::new("p-1"), amount("0.01")),
            (PayableAccountId::new("p-3"), amount("999999999999.99")),
        ];
        let mut executor = NoTransaction;
        let identity = &mut executor as *mut NoTransaction as usize;
        let result =
            update_many(&writer, &deltas, "actor", InvoicingDirection::Apply, &mut executor).await.unwrap();
        assert_eq!(result.applied, vec![deltas[0].0.clone(), deltas[2].0.clone()]);
        assert_eq!(result.rejected, vec![deltas[1].0.clone()]);
        let calls = writer.calls.lock().unwrap();
        assert_eq!(calls.len(), 3);
        for (call, (id, amount)) in calls.iter().zip(&deltas) {
            assert_eq!(call.executor, identity);
            assert_eq!(call.filter.get_str("id").unwrap(), id.as_ref());
            assert_eq!(call.filter.get_i64("deleted_at").unwrap(), 0);
            let expected_amount = amount_bson(amount).unwrap();
            let limit = call.filter.get_document("$expr").unwrap().get_array("$lte").unwrap();
            let add = limit[0].as_document().unwrap().get_array("$add").unwrap();
            assert_eq!(add, &vec![Bson::String("$invoiced_total".into()), expected_amount]);
            assert_eq!(limit[1], Bson::String("$invoiceable_total".into()));
            let set = call.pipeline[0].get_document("$set").unwrap();
            assert_eq!(set.get_str("updated_by").unwrap(), "actor");
            assert_eq!(set.get_document("invoiced_total").unwrap().get_array("$add").unwrap(), add);
            assert!(!set.contains_key("status"));
        }
    }

    #[tokio::test]
    async fn revert_uses_existing_progress_guard_and_subtraction() {
        let writer = RecordingWriter::new([Ok(true), Ok(false)]);
        let deltas = vec![
            (ReceivableAccountId::new("r-1"), amount("12.34")),
            (ReceivableAccountId::new("r-2"), amount("0.01")),
        ];
        let result = update_many(&writer, &deltas, "actor", InvoicingDirection::Revert, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(result.applied, vec![deltas[0].0.clone()]);
        assert_eq!(result.rejected, vec![deltas[1].0.clone()]);
        let calls = writer.calls.lock().unwrap();
        for (call, (id, amount)) in calls.iter().zip(&deltas) {
            let expected = vec![Bson::String("$invoiced_total".into()), amount_bson(amount).unwrap()];
            assert_eq!(call.filter.get_str("id").unwrap(), id.as_ref());
            assert_eq!(call.filter.get_i64("deleted_at").unwrap(), 0);
            assert_eq!(call.filter.get_document("$expr").unwrap().get_array("$gte").unwrap(), &expected);
            let set = call.pipeline[0].get_document("$set").unwrap();
            assert_eq!(
                set.get_document("invoiced_total").unwrap().get_array("$subtract").unwrap(),
                &expected
            );
        }
    }

    #[tokio::test]
    async fn first_storage_error_stops_later_writes_for_both_directions() {
        for direction in [InvoicingDirection::Apply, InvoicingDirection::Revert] {
            let writer = RecordingWriter::new([
                Ok(true),
                Err(Error::EntityMetadataOutOfRange("invoice-progress")),
                Ok(true),
            ]);
            let deltas = vec![
                (PayableAccountId::new("p-1"), amount("1.00")),
                (PayableAccountId::new("p-2"), amount("2.00")),
                (PayableAccountId::new("p-3"), amount("3.00")),
            ];
            let result = update_many(&writer, &deltas, "actor", direction, &mut NoTransaction).await;
            assert!(matches!(result, Err(Error::EntityMetadataOutOfRange("invoice-progress"))));
            let calls = writer.calls.lock().unwrap();
            assert_eq!(calls.len(), 2);
            assert_eq!(calls[0].filter.get_str("id").unwrap(), "p-1");
            assert_eq!(calls[1].filter.get_str("id").unwrap(), "p-2");
        }
    }

    #[tokio::test]
    async fn empty_batches_do_not_call_writer() {
        for direction in [InvoicingDirection::Apply, InvoicingDirection::Revert] {
            let writer = RecordingWriter::new([]);
            let result =
                update_many::<PayableAccountId>(&writer, &[], "actor", direction, &mut NoTransaction)
                    .await
                    .unwrap();
            assert!(result.applied.is_empty());
            assert!(result.rejected.is_empty());
            assert!(writer.calls.lock().unwrap().is_empty());
        }
    }
}
