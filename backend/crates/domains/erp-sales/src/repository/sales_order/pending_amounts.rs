//! 首次生效前的列表金额读取。

use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use erp_core::money::Amount;
use mongodb::Database;
use mongodb::bson::{Document, doc};
use persistence_core::{Executor, Result, aggregate};
use serde::Deserialize;

use super::working_copy::active_copy_filter;
use super::{
    SALES_ORDER_SUBMISSIONS, SALES_ORDER_WORKING_COPIES, SALES_ORDERS, SalesOrderDomainRepository,
    SalesOrderExt,
};
use crate::entity::sales_order::WorkingPurpose;

/// 首次生效前供列表展示的销售金额。
#[derive(Debug, Deserialize)]
pub struct PendingSalesOrderAmounts {
    /// 稳定销售单身份。
    pub id: String,
    /// 含税金额。
    pub gross_amount: Amount,
    /// 不含税金额。
    pub net_amount: Amount,
    /// 税额。
    pub tax_amount: Amount,
}

impl SalesOrderDomainRepository<'_> {
    /// 为同一快照内已授权的列表行批量读取首次生效前的金额。
    ///
    /// 优先使用有明细的有效首次工作副本，否则使用提交序号最大的快照。
    /// 已有正式版本或没有任一金额来源的单据不出现在结果中。
    ///
    /// # Errors
    /// 聚合查询或结果解码失败时返回仓储错误。
    pub async fn pending_amounts(
        &self,
        order_ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<PendingSalesOrderAmounts>> {
        if order_ids.is_empty() {
            return Ok(Vec::new());
        }
        let collection = self.db.collection::<PendingSalesOrderAmounts>(SALES_ORDERS);
        aggregate::rows(&collection, pending_amounts_pipeline(order_ids), executor).await
    }
}

fn pending_amounts_pipeline(order_ids: &[String]) -> Vec<Document> {
    let mut active_copy = active_copy_filter(WorkingPurpose::FirstSubmission);
    active_copy.insert("deleted_at", NOT_DELETED_TIMESTAMP_BSON);
    let amounts = doc! { "_id": 0, "gross_amount": 1, "net_amount": 1, "tax_amount": 1 };

    vec![
        doc! { "$match": {
            "id": { "$in": order_ids },
            "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
            "current_revision_id": null,
        } },
        doc! { "$lookup": {
            "from": SALES_ORDER_WORKING_COPIES,
            "localField": "id",
            "foreignField": "sales_order_id",
            "pipeline": [
                { "$match": active_copy },
                { "$lookup": {
                    "from": Database::SALES_ORDER_WORKING_COPY_LINES,
                    "localField": "id",
                    "foreignField": "working_copy_id",
                    "pipeline": [
                        { "$match": { "deleted_at": NOT_DELETED_TIMESTAMP_BSON } },
                        { "$limit": 1 },
                        { "$project": { "_id": 1 } },
                    ],
                    "as": "lines",
                } },
                { "$match": { "lines.0": { "$exists": true } } },
                { "$project": amounts.clone() },
            ],
            "as": "draft",
        } },
        doc! { "$lookup": {
            "from": SALES_ORDER_SUBMISSIONS,
            "localField": "id",
            "foreignField": "sales_order_id",
            "pipeline": [
                { "$match": { "deleted_at": NOT_DELETED_TIMESTAMP_BSON } },
                { "$sort": { "submission_no": -1 } },
                { "$limit": 1 },
                { "$project": amounts },
            ],
            "as": "submission",
        } },
        doc! { "$match": { "$or": [
            { "draft.0": { "$exists": true } },
            { "submission.0": { "$exists": true } },
        ] } },
        doc! { "$replaceWith": { "$mergeObjects": [
            { "id": "$id" },
            { "$ifNull": [{ "$first": "$draft" }, { "$first": "$submission" }] },
        ] } },
    ]
}
