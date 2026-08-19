//! `delivery` 发货单仓储：列表投影查询与按物流单号查询。

use entities::common::time::Instant;
use entities::fulfillment::{Delivery, DeliveryState, DeliveryType};
use entities::ids::{SalesOrderId, WarehouseId};
use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use mongodb::bson::{doc, Document};
use mongodb::options::FindOptions;
use serde::{Deserialize, Serialize};

use super::sort_doc;
use crate::executor::Executor;
use crate::repository::{PageResult, Pagination, QueryFilter};
use crate::{mongo_ops, Repository, Result};

/// 发货单排序白名单（查询与测试共用）。
const DELIVERY_SORT_FIELDS: &[&str] = &["created_at", "shipped_at"];

/// 发货单列表投影行。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeliveryRow {
    /// 实体主键。
    pub id: String,
    /// 履约发货单号。
    pub delivery_no: String,
    /// 发货类型。
    pub delivery_type: DeliveryType,
    /// 销售单。
    pub sales_order_id: SalesOrderId,
    /// 入库仓（仓发）。
    pub warehouse_id: Option<WarehouseId>,
    /// 当前状态。
    pub status: DeliveryState,
    /// 物流承运方。
    pub carrier: Option<String>,
    /// 物流单号。
    pub tracking_no: Option<String>,
    /// 发货时间。
    pub shipped_at: Option<Instant>,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 发货单列表筛选条件。
#[derive(Debug, Clone)]
pub struct DeliveryFilter {
    /// 销售单；`None` 表示不筛选。
    pub sales_order_id: Option<SalesOrderId>,
    /// 单据状态；`None` 表示不筛选。
    pub status: Option<DeliveryState>,
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 排序字段（白名单内；`None` 默认 `created_at`）。
    pub sort_by: Option<String>,
    /// 是否升序；`false` 表示降序（默认）。
    pub sort_ascending: bool,
}

impl QueryFilter for DeliveryFilter {
    /// 转换为 MongoDB 查询条件（自动追加未删除过滤）。
    ///
    /// # 返回
    /// 返回查询条件文档。
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        if let Some(sales_order_id) = &self.sales_order_id {
            filter.insert("sales_order_id", sales_order_id.to_string());
        }
        if let Some(status) = self.status {
            filter.insert("status", status.as_str());
        }
        filter
    }
}

impl Pagination for DeliveryFilter {
    /// 返回页码与单页条数。
    ///
    /// # 返回
    /// 返回 `(page, page_size)` 元组。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

impl<'a> Repository<'a, Delivery> {
    /// 分页检索发货单列表（投影查询）。
    ///
    /// 只返回 [`DeliveryRow`] 所需的列表字段（敏感履约地址字段不进投影）；
    /// 排序字段走白名单映射（`created_at`/`shipped_at`）。
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
    pub async fn search_deliveries(
        &self,
        filter: &DeliveryFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<DeliveryRow>> {
        let options = FindOptions::builder()
            .sort(sort_doc(
                filter.sort_by.as_deref(),
                filter.sort_ascending,
                DELIVERY_SORT_FIELDS,
            ))
            .skip(filter.skip())
            .limit(filter.limit())
            .projection(delivery_projection())
            .build();
        let collection = self.collection().clone_with_type::<DeliveryRow>();
        let items = mongo_ops::find_many(&collection, filter.to_doc(), options, executor).await?;
        let total = mongo_ops::count_documents(&self.collection(), filter.to_doc(), executor).await?;
        Ok(PageResult {
            items,
            total: total as i64,
        })
    }
}

/// 发货单列表投影字段（履约地址敏感字段不进投影）。
///
/// # 返回
/// 返回投影条件文档。
fn delivery_projection() -> Document {
    doc! {
        "id": 1,
        "delivery_no": 1,
        "delivery_type": 1,
        "sales_order_id": 1,
        "warehouse_id": 1,
        "status": 1,
        "carrier": 1,
        "tracking_no": 1,
        "shipped_at": 1,
        "version": 1,
        "created_at": 1,
    }
}

#[cfg(test)]
mod tests {
    use super::{delivery_projection, sort_doc, DELIVERY_SORT_FIELDS};
    use mongodb::bson::doc;

    #[test]
    fn projection_excludes_sensitive_and_purchase_fields() {
        let projection = delivery_projection();
        let keys: Vec<&str> = projection.keys().map(String::as_str).collect();
        assert_eq!(
            keys,
            vec![
                "id",
                "delivery_no",
                "delivery_type",
                "sales_order_id",
                "warehouse_id",
                "status",
                "carrier",
                "tracking_no",
                "shipped_at",
                "version",
                "created_at",
            ],
            "列表投影必须精确等于 Row 字段集合"
        );
        assert!(
            !projection.contains_key("purchase_order_id"),
            "采购来源字段不得进入发货列表投影"
        );
        assert!(
            !projection.contains_key("address_snapshot_encrypted"),
            "敏感履约地址不得进入发货列表投影"
        );
        assert!(
            !projection.contains_key("address_snapshot_fingerprint"),
            "地址指纹不得进入发货列表投影"
        );
    }

    #[test]
    fn sort_whitelist_maps_shipped_at_and_defaults_to_created_at() {
        assert_eq!(
            sort_doc(Some("shipped_at"), true, DELIVERY_SORT_FIELDS),
            doc! { "shipped_at": 1 },
            "白名单内字段按调用方方向排序"
        );
        assert_eq!(
            sort_doc(Some("未知字段"), false, DELIVERY_SORT_FIELDS),
            doc! { "created_at": -1 },
            "白名单外的字段名回落默认排序并保留调用方方向"
        );
    }
}
