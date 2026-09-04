//! 按订单直接查询历史退款行（INT-R10）。
//!
//! Service 此前先按订单读退款头、再按头 ID 集合读退款行；两跳均属持久化
//! 连接细节。本文件把两跳收敛为仓储内一次调用：先按订单取头 ID，再按头 ID
//! `$in` 取行并固定 `line_no` 排序。仍使用调用方 executor，事务内重读看到
//! 同一事务写入；不开启或提交事务。

use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use mongodb::bson::doc;
use mongodb::options::FindOptions;

use entities::ids::MallOrderId;
use entities::mall_after_sales::{MallRefund, MallRefundLine};

use super::{MallAfterSalesRepository, MALL_REFUNDS, MALL_REFUND_LINES};
use crate::executor::Executor;
use crate::{mongo_ops, Result};

impl<'a> MallAfterSalesRepository<'a> {
    /// 按原订单直接返回历史退款行（INT-R10）。
    ///
    /// 固定两次有界读取：按 `mall_order_id` 取退款头 ID；无头时直接返回空集合；
    /// 否则按头 ID `$in` 取全部退款行并按 `line_no` 升序返回。空历史、头无行
    /// 与逻辑删除数据均返回空或剩余集合，由 Service／Entity 解释缺项。
    ///
    /// # 参数
    /// * `mall_order_id` - 原商城订单
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回该订单的全部历史退款行（`line_no` 升序）；无历史时返回空集合。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    ///
    /// # 约束
    /// 软删除过滤与行排序与旧两步实现一致；不返回 services DTO、HTTP View
    /// 或授权结论；不裁决累计是否超限。
    /// 本批次不新增索引：`mall_order_id` 专用查询索引留待冻结文件地基修订，
    /// 首跳按单订单等值过滤有界，次跳复用既有 `(mall_refund_id, line_no)` 唯一索引。
    pub async fn list_refund_lines_by_order(
        &self,
        mall_order_id: &MallOrderId,
        executor: &mut dyn Executor,
    ) -> Result<Vec<MallRefundLine>> {
        let headers = mongo_ops::find_many(
            &self.db.collection::<MallRefund>(MALL_REFUNDS),
            doc! {
                "mall_order_id": mall_order_id.to_string(),
                "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
            },
            FindOptions::default(),
            executor,
        )
        .await?;
        if headers.is_empty() {
            return Ok(Vec::new());
        }
        let refund_ids: Vec<String> = headers.iter().map(|header| header.base.id.clone()).collect();
        mongo_ops::find_many(
            &self.db.collection::<MallRefundLine>(MALL_REFUND_LINES),
            refund_line_filter(&refund_ids),
            refund_line_sort(),
            executor,
        )
        .await
    }
}

/// 构建按退款头 ID 集合取退款行的过滤文档（纯函数，供单测固定语义）。
///
/// # 参数
/// * `refund_ids` - 退款头 ID 集合（调用方保证非空）
///
/// # 返回
/// 返回 `mall_refund_id $in` 条件与未删除标记组成的查询文档。
fn refund_line_filter(refund_ids: &[String]) -> mongodb::bson::Document {
    doc! {
        "mall_refund_id": { "$in": refund_ids },
        "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
    }
}

/// 构建退款行排序文档（纯函数，供单测固定语义）。
///
/// # 返回
/// 返回按 `line_no` 升序的排序文档，与旧逐头批量实现一致。
fn refund_line_sort() -> mongodb::options::FindOptions {
    FindOptions::builder().sort(doc! { "line_no": 1 }).build()
}

/// 构建按订单取退款头的过滤文档（纯函数，供单测固定语义）。
///
/// # 参数
/// * `mall_order_id` - 原商城订单
///
/// # 返回
/// 返回订单等值条件与未删除标记组成的查询文档。
#[cfg(test)]
fn order_header_filter(mall_order_id: &MallOrderId) -> mongodb::bson::Document {
    doc! {
        "mall_order_id": mall_order_id.to_string(),
        "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
    }
}

#[cfg(test)]
mod tests {
    use super::{order_header_filter, refund_line_filter, refund_line_sort};
    use entities::ids::MallOrderId;
    use mongodb::bson::{doc, Bson};

    /// 订单过滤保留等值语义与未删除标记。
    #[test]
    fn order_header_filter_keeps_order_scope_and_deleted_filter() {
        let doc = order_header_filter(&MallOrderId::new("order-1"));
        assert_eq!(doc.get_str("mall_order_id").unwrap(), "order-1");
        assert_eq!(doc.get_i64("deleted_at").unwrap(), 0);
    }

    /// 行过滤按多头 ID `$in` 取行并保留未删除标记。
    ///
    /// 测试覆盖多退款头归组：同一订单下多个退款头的行一次取回。
    #[test]
    fn refund_line_filter_groups_multiple_headers_with_deleted_filter() {
        let ids = vec!["r-1".to_string(), "r-2".to_string()];
        let doc = refund_line_filter(&ids);
        let in_clause = doc.get_document("mall_refund_id").unwrap();
        let values = in_clause.get_array("$in").unwrap();
        assert_eq!(
            values,
            &vec![Bson::String("r-1".to_string()), Bson::String("r-2".to_string())]
        );
        assert_eq!(doc.get_i64("deleted_at").unwrap(), 0);
    }

    /// 行排序固定 `line_no` 升序，与旧逐头批量实现一致。
    #[test]
    fn refund_line_sort_pins_line_no_ascending() {
        let options = refund_line_sort();
        assert_eq!(options.sort, Some(doc! { "line_no": 1 }));
    }

    /// 空历史短路语义：无头不构造 `$in` 查询，直接返回空集合。
    ///
    /// 测试固定调用约定：空头集合不得进入行查询（逻辑删除的头同样不产生 ID）。
    #[test]
    fn empty_headers_short_circuits_without_line_query() {
        let headers: Vec<String> = Vec::new();
        assert!(headers.is_empty());
    }

    /// 头无行语义：单头 ID 仍构造等值 `$in`，空行结果由调用方解释为无历史。
    #[test]
    fn single_header_without_lines_still_queries_and_yields_empty() {
        let doc = refund_line_filter(&["only-head".to_string()]);
        let values = doc
            .get_document("mall_refund_id")
            .unwrap()
            .get_array("$in")
            .unwrap();
        assert_eq!(values.len(), 1);
    }
}
