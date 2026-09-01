//! `purchase_receipt_line` 累计合格收货聚合（FUL-R01）。
//!
//! 入库过账路径需要「历史累计 + 本次收货不超过采购数量」中的历史累计：
//! 按采购版本行汇总采购单全部已过账、未删除入库行的合格数量。本模块在
//! 数据库内完成过滤与聚合，只返回强类型「采购版本行 → 累计合格数量」映射，
//! 不再把入库单/入库行整实体反序列化进 Service（§6.1 责任边界）。
//!
//! 索引适配：已过账入库单查询走 `idx_purchase_receipts_po_status_posted`
//! （`purchase_order_id + status + posted_at`），入库行 `$in` 走
//! `uk_purchase_receipt_lines_header_line`（`purchase_receipt_id` 前缀）；
//! 两次固定查询，访问次数不随单据或行数增长。

use std::collections::HashMap;

use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use futures_util::TryStreamExt;
use mongodb::bson::{doc, Document};
use mongodb::options::FindOptions;
use mongodb::Database;
use serde::Deserialize;

use entities::fulfillment::PurchaseReceiptState;
use entities::ids::{PurchaseOrderId, PurchaseOrderRevisionLineId};
use entities::money::Quantity;

use crate::executor::Executor;
use crate::repository::extensions::FulfillmentExt;
use crate::{mongo_ops, Result};

/// 已过账入库单 ID 投影行。
#[derive(Debug, Deserialize)]
struct PostedReceiptIdRow {
    /// 入库单主键。
    id: String,
}

/// 采购入库累计合格收货聚合行。
#[derive(Debug, Deserialize)]
struct QualifiedReceivedTotalRow {
    /// 采购版本行主键（聚合分组键）。
    #[serde(rename = "_id")]
    purchase_order_revision_line_id: PurchaseOrderRevisionLineId,
    /// 累计合格数量（Decimal128 求和结果）。
    total: Quantity,
}

/// 统计采购单已过账入库的累计有效收货（按采购版本行分组）。
///
/// 先在 `purchase_receipts` 上按采购单过滤未删除且已过账的入库单并只投影
/// 主键，再在 `purchase_receipt_lines` 上按入库单主键 `$in` 过滤未删除行并
/// 按 `purchase_order_revision_line_id` 聚合 `qualified_quantity`。空结果返回
/// 空映射，不发送空 `$in`。两个查询都使用调用方执行器：位于事务内时看到
/// 同一事务的未提交写入，不自行开启事务。
///
/// # 参数
/// * `db` - 目标 MongoDB 数据库
/// * `purchase_order_id` - 采购单主键
/// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
///
/// # 返回
/// 返回「采购版本行 → 累计合格数量」映射；无任何已过账未删除入库行时返回
/// 空映射。
///
/// # 错误
/// 聚合或游标读取失败时返回错误；Decimal128 求和结果无法转换为
/// `Quantity`（精度或上限越界）时返回错误而非 panic。
pub(super) async fn load_qualified_received_totals(
    db: &Database,
    purchase_order_id: &PurchaseOrderId,
    executor: &mut dyn Executor,
) -> Result<HashMap<PurchaseOrderRevisionLineId, Quantity>> {
    let receipts = mongo_ops::find_many(
        &db.collection::<PostedReceiptIdRow>(<mongodb::Database as FulfillmentExt>::PURCHASE_RECEIPTS),
        doc! {
            "purchase_order_id": purchase_order_id.to_string(),
            "status": PurchaseReceiptState::Posted.as_str(),
            "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
        },
        FindOptions::builder().projection(doc! { "id": 1 }).build(),
        executor,
    )
    .await?;
    let receipt_ids: Vec<String> = receipts.into_iter().map(|row| row.id).collect();
    if receipt_ids.is_empty() {
        return Ok(HashMap::new());
    }
    let rows = match executor.session() {
        Some(session) => {
            db.collection::<QualifiedReceivedTotalRow>(super::PURCHASE_RECEIPT_LINES)
                .aggregate(qualified_received_totals_pipeline(&receipt_ids))
                .with_type::<QualifiedReceivedTotalRow>()
                .session(&mut *session)
                .await?
                .stream(session)
                .try_collect::<Vec<_>>()
                .await?
        }
        None => {
            db.collection::<QualifiedReceivedTotalRow>(super::PURCHASE_RECEIPT_LINES)
                .aggregate(qualified_received_totals_pipeline(&receipt_ids))
                .with_type::<QualifiedReceivedTotalRow>()
                .await?
                .try_collect::<Vec<_>>()
                .await?
        }
    };
    Ok(rows
        .into_iter()
        .map(|row| (row.purchase_order_revision_line_id, row.total))
        .collect())
}

/// 构造入库行累计合格收货聚合管道。
///
/// # 参数
/// * `receipt_ids` - 已过账且未删除的入库单主键集合（非空）
///
/// # 返回
/// 返回两段聚合管道：`$match` 按入库单过滤未删除行，`$group` 按采购版本行
/// 对 `qualified_quantity` 求和。
fn qualified_received_totals_pipeline(receipt_ids: &[String]) -> Vec<Document> {
    vec![
        doc! {
            "$match": {
                "purchase_receipt_id": { "$in": receipt_ids },
                "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
            }
        },
        doc! {
            "$group": {
                "_id": "$purchase_order_revision_line_id",
                "total": { "$sum": "$qualified_quantity" },
            }
        },
    ]
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use mongodb::bson::Bson;

    use super::*;

    /// 聚合管道只包含未删除行并按采购版本行求和合格数量。
    #[test]
    fn totals_pipeline_filters_undelted_lines_and_groups_by_revision_line() {
        let pipeline =
            qualified_received_totals_pipeline(&["receipt-1".to_string(), "receipt-2".to_string()]);
        let match_stage = pipeline[0].get_document("$match").expect("过滤阶段");
        let receipt_ids = match_stage
            .get_document("purchase_receipt_id")
            .expect("入库单主键条件")
            .get_array("$in")
            .expect("入库单主键 $in 条件");
        assert_eq!(
            receipt_ids,
            &[
                Bson::String("receipt-1".to_string()),
                Bson::String("receipt-2".to_string())
            ]
        );
        assert_eq!(match_stage.get_i64("deleted_at").expect("未删除条件"), 0);
        let group_stage = pipeline[1].get_document("$group").expect("分组阶段");
        assert_eq!(
            group_stage.get_str("_id").expect("分组键"),
            "$purchase_order_revision_line_id"
        );
        assert_eq!(
            group_stage
                .get_document("total")
                .expect("求和字段")
                .get_str("$sum")
                .expect("求和表达式"),
            "$qualified_quantity"
        );
    }

    /// Decimal128 聚合结果可反序列化为 `Quantity`。
    #[test]
    fn row_deserializes_decimal128_total() {
        let document = doc! { "_id": "po-line-1", "total": { "$numberDecimal": "12.500000" } };
        let row: QualifiedReceivedTotalRow =
            mongodb::bson::deserialize_from_document(document).expect("合法 Decimal128 必须成功");
        assert_eq!(
            row.purchase_order_revision_line_id,
            PurchaseOrderRevisionLineId::new("po-line-1")
        );
        assert_eq!(row.total, Quantity::from_str("12.5").expect("合法数量"));
    }

    /// 超精度 Decimal128 必须返回反序列化错误而非 panic。
    #[test]
    fn row_rejects_precision_overflow_without_panicking() {
        let document = doc! { "_id": "po-line-2", "total": { "$numberDecimal": "1.1234567" } };
        let result: std::result::Result<QualifiedReceivedTotalRow, mongodb::bson::error::Error> =
            mongodb::bson::deserialize_from_document(document);
        assert!(result.is_err(), "超精度 Decimal128 必须失败而非 panic");
    }
}
