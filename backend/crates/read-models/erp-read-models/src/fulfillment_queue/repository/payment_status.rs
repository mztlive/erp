//! 已授权工作台当前页的付款与放行摘要，一次聚合读取，不逐项查询详情。

use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use erp_core::money::Amount;
use erp_finance::repository::PayableExt;
use erp_workflow::entity::work_item::{WorkItemStatus, WorkItemType};
use futures_util::TryStreamExt;
use mongodb::Database;
use mongodb::bson::{Document, doc};
use persistence_core::{Executor, Result};
use serde::Deserialize;

use super::FulfillmentQueueRepository;
use super::pipeline::fulfillment_facts_pipeline;

/// 仅包含当前任务的付款金额与放行码，不返回付款申请和银行资料。
#[derive(Debug, Deserialize)]
pub(crate) struct FulfillmentPaymentStatusRow {
    pub work_item_id: String,
    pub gate_state: String,
    pub paid_amount: Option<Amount>,
    pub open_amount: Option<Amount>,
}

impl FulfillmentQueueRepository<'_> {
    /// 批量读取已授权当前页的履约付款状态。
    ///
    /// # 参数
    /// `work_item_ids` 必须来自完成对象授权的当前页；`executor` 沿用列表读取事务。
    /// # 返回
    /// 返回仍匹配开放任务及草稿版本的摘要；空输入不访问数据库。
    /// # 错误
    /// 聚合执行或金额反序列化失败时返回错误，不伪造未付款或可发货状态。
    pub(crate) async fn payment_statuses(
        &self,
        work_item_ids: &[&str],
        executor: &mut dyn Executor,
    ) -> Result<Vec<FulfillmentPaymentStatusRow>> {
        if work_item_ids.is_empty() {
            return Ok(Vec::new());
        }
        let mut stages = fulfillment_facts_pipeline(doc! {
            "id": { "$in": work_item_ids },
            "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
            "status": WorkItemStatus::Open.as_str(),
            "work_item_type": WorkItemType::FulfillmentOperation.as_str(),
        });
        stages.push(account_lookup());
        stages.push(doc! { "$project": {
            "_id": 0, "work_item_id": "$id", "gate_state": 1,
            "paid_amount": { "$arrayElemAt": ["$_payment_accounts.settled_total", 0] },
            "open_amount": { "$arrayElemAt": ["$_payment_accounts.open_total", 0] },
        } });
        let collection = self.collection();
        let query = collection.aggregate(stages).with_type::<FulfillmentPaymentStatusRow>();
        let rows = match executor.session() {
            Some(session) => query.session(&mut *session).await?.stream(session).try_collect().await?,
            None => query.await?.try_collect().await?,
        };
        Ok(rows)
    }
}

/// 按采购来源唯一索引读取应付余额，付款口径与履约付款详情相同。
fn account_lookup() -> Document {
    doc! { "$lookup": {
        "from": <Database as PayableExt>::PAYABLE_ACCOUNTS,
        "let": { "purchase_order_id": "$operation.purchase_order_id" },
        "pipeline": [
            { "$match": {
                "deleted_at": NOT_DELETED_TIMESTAMP_BSON, "source_type": "purchase_order",
                "$expr": { "$eq": ["$source_document_id", "$$purchase_order_id"] },
            } },
            { "$project": { "_id": 0, "settled_total": 1, "open_total": 1 } },
        ],
        "as": "_payment_accounts",
    } }
}
