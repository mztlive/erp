//! `purchase_receipt` 采购入库单仓储：列表投影查询与按入库单号身份查询。

use entities::common::time::Instant;
use entities::fulfillment::{PurchaseReceipt, PurchaseReceiptState};
use entities::ids::{PurchaseOrderId, WarehouseId};
use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use mongodb::bson::{doc, Document};
use mongodb::options::FindOptions;
use serde::{Deserialize, Serialize};

use super::sort_doc;
use crate::executor::Executor;
use crate::repository::{PageResult, Pagination, QueryFilter};
use crate::{mongo_ops, Repository, Result};

/// 采购入库单排序白名单（查询与测试共用）。
const PURCHASE_RECEIPT_SORT_FIELDS: &[&str] = &["created_at", "posted_at"];

/// 采购入库单列表投影行（列表接口只取必要字段，禁止返回整文档）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PurchaseReceiptRow {
    /// 实体主键。
    pub id: String,
    /// 采购入库单号。
    pub receipt_no: String,
    /// 来源采购单。
    pub purchase_order_id: PurchaseOrderId,
    /// 入库仓。
    pub warehouse_id: WarehouseId,
    /// 当前状态。
    pub status: PurchaseReceiptState,
    /// 入库过账时间。
    pub posted_at: Option<Instant>,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 采购入库单列表筛选条件。
#[derive(Debug, Clone)]
pub struct PurchaseReceiptFilter {
    /// 来源采购单；`None` 表示不筛选。
    pub purchase_order_id: Option<PurchaseOrderId>,
    /// 单据状态；`None` 表示不筛选。
    pub status: Option<PurchaseReceiptState>,
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 排序字段（白名单内；`None` 默认 `created_at`）。
    pub sort_by: Option<String>,
    /// 是否升序；`false` 表示降序（默认）。
    pub sort_ascending: bool,
}

impl QueryFilter for PurchaseReceiptFilter {
    /// 转换为 MongoDB 查询条件（自动追加未删除过滤）。
    ///
    /// # 返回
    /// 返回查询条件文档。
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        if let Some(purchase_order_id) = &self.purchase_order_id {
            filter.insert("purchase_order_id", purchase_order_id.to_string());
        }
        if let Some(status) = self.status {
            filter.insert("status", status.as_str());
        }
        filter
    }
}

impl Pagination for PurchaseReceiptFilter {
    /// 返回页码与单页条数。
    ///
    /// # 返回
    /// 返回 `(page, page_size)` 元组。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

impl<'a> Repository<'a, PurchaseReceipt> {
    /// 分页检索采购入库单列表（投影查询）。
    ///
    /// 只返回 [`PurchaseReceiptRow`] 所需的列表字段，不加载整文档；排序字段
    /// 走白名单映射（`created_at`/`posted_at`），白名单外的字段名回落默认值。
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
    #[tracing::instrument(
        name = "repository.fulfillment.search_purchase_receipts",
        skip_all,
        fields(
            layer = "repository",
            domain = "fulfillment",
            db.system.name = "mongodb",
            db.collection.name = "purchase_receipts",
            db.operation.name = "search"
        )
    )]
    pub async fn search_purchase_receipts(
        &self,
        filter: &PurchaseReceiptFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<PurchaseReceiptRow>> {
        let options = FindOptions::builder()
            .sort(sort_doc(
                filter.sort_by.as_deref(),
                filter.sort_ascending,
                PURCHASE_RECEIPT_SORT_FIELDS,
            ))
            .skip(filter.skip())
            .limit(filter.limit())
            .projection(purchase_receipt_projection())
            .build();
        let collection = self.collection().clone_with_type::<PurchaseReceiptRow>();
        let items = mongo_ops::find_many(&collection, filter.to_doc(), options, executor).await?;
        let total = mongo_ops::count_documents(&self.collection(), filter.to_doc(), executor).await?;
        Ok(PageResult {
            items,
            total: total as i64,
        })
    }

    /// 按采购入库单号查找入库单（唯一单号，详情查询）。
    ///
    /// # 参数
    /// * `receipt_no` - 采购入库单号
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配的未删除入库单；无匹配时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    pub async fn find_by_receipt_no(
        &self,
        receipt_no: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<PurchaseReceipt>> {
        self.find_one_by_field("receipt_no", receipt_no, executor).await
    }
}

/// 采购入库单列表投影字段。
///
/// # 返回
/// 返回投影条件文档。
fn purchase_receipt_projection() -> Document {
    doc! {
        "id": 1,
        "receipt_no": 1,
        "purchase_order_id": 1,
        "warehouse_id": 1,
        "status": 1,
        "posted_at": 1,
        "version": 1,
        "created_at": 1,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        purchase_receipt_projection, sort_doc, Pagination, PurchaseReceiptFilter, QueryFilter,
        PURCHASE_RECEIPT_SORT_FIELDS,
    };
    use mongodb::bson::doc;

    use entities::fulfillment::PurchaseReceiptState;
    use entities::ids::PurchaseOrderId;

    #[test]
    fn receipt_filter_applies_optional_fields_and_deleted_filter() {
        let filter = PurchaseReceiptFilter {
            purchase_order_id: Some(PurchaseOrderId::new("po-1")),
            status: Some(PurchaseReceiptState::Posted),
            page: 2,
            page_size: 10,
            sort_by: None,
            sort_ascending: false,
        };

        let document = filter.to_doc();
        assert_eq!(document.get_i64("deleted_at").unwrap(), 0);
        assert_eq!(document.get_str("purchase_order_id").unwrap(), "po-1");
        assert_eq!(document.get_str("status").unwrap(), "POSTED");
        assert_eq!(filter.skip(), 10);
        assert_eq!(filter.limit(), 10);
    }

    #[test]
    fn projection_and_sort_whitelist_contract() {
        let projection = purchase_receipt_projection();
        let keys: Vec<&str> = projection.keys().map(String::as_str).collect();
        assert_eq!(
            keys,
            vec![
                "id",
                "receipt_no",
                "purchase_order_id",
                "warehouse_id",
                "status",
                "posted_at",
                "version",
                "created_at",
            ],
            "列表投影必须精确等于 Row 字段集合"
        );

        assert_eq!(
            sort_doc(Some("posted_at"), true, PURCHASE_RECEIPT_SORT_FIELDS),
            doc! { "posted_at": 1 },
            "白名单内字段按调用方方向排序"
        );
        assert_eq!(
            sort_doc(Some("未知字段"), false, PURCHASE_RECEIPT_SORT_FIELDS),
            doc! { "created_at": -1 },
            "白名单外的字段名回落默认排序并保留调用方方向"
        );
    }
}
