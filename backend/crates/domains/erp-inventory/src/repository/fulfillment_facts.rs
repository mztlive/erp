//! 库存拥有的履约入库预占来源查询。

use erp_core::ids::PurchaseReceiptLineId;
use persistence_core::{Executor, Repository, Result};

use crate::StockReservation;

/// 履约入库预占来源查询。
#[allow(async_fn_in_trait)]
pub trait StockReservationRepositoryFulfillmentExt {
    /// 查询入库行形成的库存预占。
    /// 返回全部匹配且未删除的库存预占。
    /// `receipt_line_ids` 为入库行主键集合；沿用调用方执行器 `executor`。
    /// 传播 MongoDB 查询或游标读取错误。
    async fn list_stock_reservations_for_receipt_lines(
        &self,
        receipt_line_ids: &[PurchaseReceiptLineId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<StockReservation>>;
}

impl StockReservationRepositoryFulfillmentExt for Repository<'_, StockReservation> {
    async fn list_stock_reservations_for_receipt_lines(
        &self,
        receipt_line_ids: &[PurchaseReceiptLineId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<StockReservation>> {
        if receipt_line_ids.is_empty() {
            return Ok(Vec::new());
        }
        let ids = receipt_line_ids.iter().map(ToString::to_string).collect::<Vec<_>>();
        self.find_many(
            mongodb::bson::doc! {
                "source_receipt_line_id": { "$in": ids },
                "deleted_at": entity_core::NOT_DELETED_TIMESTAMP_BSON,
            },
            executor,
        )
        .await
    }
}
