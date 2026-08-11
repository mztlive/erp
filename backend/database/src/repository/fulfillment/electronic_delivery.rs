//! `electronic_delivery` 电子交付记录仓储：列表投影查询。

use entities::common::time::Instant;
use entities::fulfillment::{ElectronicDelivery, ElectronicDeliveryState, FulfillmentResult};
use entities::ids::{PurchaseOrderId, SalesOrderLineId};
use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use mongodb::bson::{doc, Document};
use mongodb::options::FindOptions;
use serde::{Deserialize, Serialize};

use super::sort_doc;
use crate::executor::Executor;
use crate::repository::{PageResult, Pagination, QueryFilter};
use crate::{mongo_ops, Repository, Result};

/// 电子交付记录排序白名单（查询与测试共用）。
const ELECTRONIC_DELIVERY_SORT_FIELDS: &[&str] = &["occurred_at", "recorded_at", "created_at"];

/// 电子交付记录列表投影行。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ElectronicDeliveryRow {
    /// 实体主键。
    pub id: String,
    /// 履约记录号。
    pub fulfillment_no: String,
    /// 销售责任明细。
    pub sales_order_line_id: SalesOrderLineId,
    /// 采购单。
    pub purchase_order_id: PurchaseOrderId,
    /// 交付数量。
    pub quantity: entities::money::Quantity,
    /// 履约结果。
    pub result: FulfillmentResult,
    /// 当前状态。
    pub status: ElectronicDeliveryState,
    /// 实际交付时间。
    pub occurred_at: Instant,
    /// ERP 记录时间。
    pub recorded_at: Instant,
    /// 乐观锁版本。
    pub version: u64,
}

/// 电子交付记录列表筛选条件。
#[derive(Debug, Clone)]
pub struct ElectronicDeliveryFilter {
    /// 销售责任明细；`None` 表示不筛选。
    pub sales_order_line_id: Option<SalesOrderLineId>,
    /// 记录状态；`None` 表示不筛选。
    pub status: Option<ElectronicDeliveryState>,
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 排序字段（白名单内；`None` 默认 `occurred_at`）。
    pub sort_by: Option<String>,
    /// 是否升序；`false` 表示降序（默认）。
    pub sort_ascending: bool,
}

impl QueryFilter for ElectronicDeliveryFilter {
    /// 转换为 MongoDB 查询条件（自动追加未删除过滤）。
    ///
    /// # 返回
    /// 返回查询条件文档。
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        if let Some(sales_order_line_id) = &self.sales_order_line_id {
            filter.insert("sales_order_line_id", sales_order_line_id.to_string());
        }
        if let Some(status) = self.status {
            filter.insert("status", status.as_str());
        }
        filter
    }
}

impl Pagination for ElectronicDeliveryFilter {
    /// 返回页码与单页条数。
    ///
    /// # 返回
    /// 返回 `(page, page_size)` 元组。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

impl<'a> Repository<'a, ElectronicDelivery> {
    /// 分页检索电子交付记录列表（投影查询）。
    ///
    /// 只返回 [`ElectronicDeliveryRow`] 所需的列表字段（交付对象快照及其指纹
    /// 不进投影）；排序字段走白名单映射（`occurred_at`/`recorded_at`/`created_at`）。
    ///
    /// # 参数
    /// * `filter` - 筛选与分页条件
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回当前页投影行与满足筛选条件的总数。
    ///
    /// # 错误
    /// 当 MongoDB 查询、游标读取或计数失败时返回错误。
    pub async fn search_electronic_deliveries(
        &self,
        filter: &ElectronicDeliveryFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<ElectronicDeliveryRow>> {
        let options = FindOptions::builder()
            .sort(sort_doc(
                filter.sort_by.as_deref(),
                filter.sort_ascending,
                ELECTRONIC_DELIVERY_SORT_FIELDS,
            ))
            .skip(filter.skip())
            .limit(filter.limit())
            .projection(electronic_delivery_projection())
            .build();
        let collection = self.collection().clone_with_type::<ElectronicDeliveryRow>();
        let items = mongo_ops::find_many(&collection, filter.to_doc(), options, executor).await?;
        let total = mongo_ops::count_documents(&self.collection(), filter.to_doc(), executor).await?;
        Ok(PageResult {
            items,
            total: total as i64,
        })
    }
}

/// 电子交付记录列表投影字段（交付对象快照及指纹不进投影）。
///
/// # 返回
/// 返回投影条件文档。
fn electronic_delivery_projection() -> Document {
    doc! {
        "id": 1,
        "fulfillment_no": 1,
        "sales_order_line_id": 1,
        "purchase_order_id": 1,
        "quantity": 1,
        "result": 1,
        "status": 1,
        "occurred_at": 1,
        "recorded_at": 1,
        "version": 1,
    }
}

#[cfg(test)]
mod tests {
    use super::{electronic_delivery_projection, sort_doc, ELECTRONIC_DELIVERY_SORT_FIELDS};
    use mongodb::bson::doc;

    #[test]
    fn projection_excludes_snapshot_and_allocation_fields() {
        let projection = electronic_delivery_projection();
        let keys: Vec<&str> = projection.keys().map(String::as_str).collect();
        assert_eq!(
            keys,
            vec![
                "id",
                "fulfillment_no",
                "sales_order_line_id",
                "purchase_order_id",
                "quantity",
                "result",
                "status",
                "occurred_at",
                "recorded_at",
                "version",
            ],
            "列表投影必须精确等于 Row 字段集合"
        );
        assert!(
            !projection.contains_key("recipient_snapshot"),
            "交付对象快照不得进入电子交付列表投影"
        );
        assert!(
            !projection.contains_key("recipient_snapshot_fingerprint"),
            "快照指纹不得进入电子交付列表投影"
        );
        assert!(
            !projection.contains_key("purchase_line_sales_allocation_id"),
            "采购分配指针不得进入电子交付列表投影"
        );
    }

    #[test]
    fn sort_whitelist_maps_occurred_at_and_defaults_to_created_at() {
        assert_eq!(
            sort_doc(Some("occurred_at"), true, ELECTRONIC_DELIVERY_SORT_FIELDS),
            doc! { "occurred_at": 1 },
            "白名单内字段按调用方方向排序"
        );
        assert_eq!(
            sort_doc(Some("未知字段"), false, ELECTRONIC_DELIVERY_SORT_FIELDS),
            doc! { "created_at": -1 },
            "白名单外的字段名回落默认排序并保留调用方方向"
        );
    }
}
