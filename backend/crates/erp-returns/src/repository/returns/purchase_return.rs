//! 采购退货单列表筛选/投影、版本集合与退货单、明细集合扩展。

use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use erp_core::common::stable::StableBase;
use erp_core::ids::{PurchaseOrderId, PurchaseReturnOrderId};
use mongodb::bson::{Document, doc};
use mongodb::options::FindOptions;
use persistence_core::{
    Executor, PageResult, Pagination, QueryFilter, Repository, Result, insert_literal_regex_filter,
};
use serde::{Deserialize, Serialize};

use super::search::{ListSort, search_projected};
use crate::entity::returns::{PurchaseReturnLine, PurchaseReturnOrder, PurchaseReturnStatus, ReturnMode};

/// 采购退货单列表投影行。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PurchaseReturnOrderRow {
    /// 实体主键。
    pub id: String,
    /// 稳定公共字段（状态/版本归属/审计人）。
    #[serde(flatten)]
    pub stable: StableBase<PurchaseReturnStatus>,
    /// 采购退货单号。
    pub purchase_return_no: String,
    /// 原采购单。
    pub purchase_order_id: String,
    /// 客户侧依据。
    pub sales_return_case_id: Option<String>,
    /// 退货模式。
    pub return_mode: ReturnMode,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 采购退货单列表筛选条件。
#[derive(Debug, Clone)]
pub struct PurchaseReturnOrderFilter {
    /// 采购退货单号模糊匹配；`None` 表示不筛选。
    pub purchase_return_no: Option<String>,
    /// 原采购单；`None` 表示不筛选。
    pub purchase_order_id: Option<PurchaseOrderId>,
    /// 已证明可见的来源采购单；`None` 表示公司范围不限制。
    pub authorized_purchase_order_ids: Option<Vec<String>>,
    /// 退货单状态；`None` 表示不筛选。
    pub status: Option<PurchaseReturnStatus>,
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 排序字段（白名单内有效，默认 `created_at`）。
    pub sort_by: Option<String>,
    /// 是否升序；`false` 表示降序（默认）。
    pub sort_ascending: bool,
}

impl Default for PurchaseReturnOrderFilter {
    /// 返回首页空筛选（`page: 1`，`page_size: 20`）。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回筛选为空、降序的首页过滤条件。
    ///
    /// # 错误
    /// 无。
    fn default() -> Self {
        Self {
            purchase_return_no: None,
            purchase_order_id: None,
            authorized_purchase_order_ids: None,
            status: None,
            page: 1,
            page_size: 20,
            sort_by: None,
            sort_ascending: false,
        }
    }
}

impl QueryFilter for PurchaseReturnOrderFilter {
    /// 转换为 MongoDB 查询条件（自动追加未删除过滤）。
    ///
    /// # 返回
    /// 返回查询条件文档。
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        insert_literal_regex_filter(&mut filter, "purchase_return_no", self.purchase_return_no.as_deref());
        if let Some(status) = self.status {
            filter.insert("status", status.as_str());
        }
        match (
            self.purchase_order_id.as_ref().map(ToString::to_string),
            self.authorized_purchase_order_ids.as_deref(),
        ) {
            (Some(purchase_order_id), None) => {
                filter.insert("purchase_order_id", purchase_order_id);
            },
            (Some(purchase_order_id), Some(authorized)) => {
                if authorized.iter().any(|id| id == &purchase_order_id) {
                    filter.insert("purchase_order_id", purchase_order_id);
                } else {
                    filter.insert("$expr", false);
                }
            },
            (None, Some(authorized)) => {
                if authorized.is_empty() {
                    filter.insert("$expr", false);
                } else {
                    filter.insert("purchase_order_id", doc! { "$in": authorized });
                }
            },
            (None, None) => {},
        }
        filter
    }
}

impl Pagination for PurchaseReturnOrderFilter {
    /// 返回页码与单页条数。
    ///
    /// # 返回
    /// 返回 `(page, page_size)` 元组。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

impl ListSort for PurchaseReturnOrderFilter {
    fn sort_by(&self) -> Option<&str> {
        self.sort_by.as_deref()
    }

    fn sort_ascending(&self) -> bool {
        self.sort_ascending
    }
}

/// 采购退货单集合仓储扩展。
#[allow(async_fn_in_trait)]
pub trait PurchaseReturnOrderRepositoryExt {
    /// 分页检索采购退货单列表（投影查询）。
    ///
    /// 只返回 [`PurchaseReturnOrderRow`] 所需的列表字段；采购退货单号支持
    /// 字面量模糊匹配。
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
    async fn search_purchase_return_orders(
        &self,
        filter: &PurchaseReturnOrderFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<PurchaseReturnOrderRow>>;

    /// 装载采购退货查询的有界身份与版本集合，用于跨页一致性校验。
    ///
    /// # 参数
    /// * `filter` - 与列表相同的筛选
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 最多 10001 行主键与版本；调用方必须整体拒绝超限。
    ///
    /// # 错误
    /// 数据库读取失败时返回仓储错误。
    ///
    /// # 关键业务约束
    /// 版本集合必须与列表同一授权条件和筛选快照。
    async fn query_purchase_return_versions(
        &self,
        filter: &PurchaseReturnOrderFilter,
        executor: &mut dyn Executor,
    ) -> Result<Vec<PurchaseReturnVersion>>;
}

impl PurchaseReturnOrderRepositoryExt for Repository<'_, PurchaseReturnOrder> {
    async fn search_purchase_return_orders(
        &self,
        filter: &PurchaseReturnOrderFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<PurchaseReturnOrderRow>> {
        search_projected(
            self,
            filter,
            purchase_return_order_projection(),
            &["purchase_return_no", "created_at"],
            executor,
        )
        .await
    }

    async fn query_purchase_return_versions(
        &self,
        filter: &PurchaseReturnOrderFilter,
        executor: &mut dyn Executor,
    ) -> Result<Vec<PurchaseReturnVersion>> {
        persistence_core::mongo_ops::find_many(
            &self.collection().clone_with_type::<PurchaseReturnVersion>(),
            filter.to_doc(),
            FindOptions::builder()
                .projection(doc! { "id": 1, "version": 1 })
                .sort(doc! { "id": 1 })
                .limit(10001)
                .build(),
            executor,
        )
        .await
    }
}

/// 跨页校验使用的采购退货身份和版本。
#[derive(Debug, serde::Deserialize, Hash)]
pub struct PurchaseReturnVersion {
    /// 采购退货单稳定主键。
    pub id: String,
    /// 采购退货单乐观锁版本。
    pub version: u64,
}

/// 采购退货明细集合仓储扩展。
#[allow(async_fn_in_trait)]
pub trait PurchaseReturnLineRepositoryExt {
    /// 批量按退货单集合取回明细（`$in` 一次取回，禁止 N+1）。
    ///
    /// # 参数
    /// * `order_ids` - 采购退货单 ID 集合
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回全部匹配明细。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    async fn find_lines_by_orders(
        &self,
        order_ids: &[PurchaseReturnOrderId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<PurchaseReturnLine>>;
}

impl PurchaseReturnLineRepositoryExt for Repository<'_, PurchaseReturnLine> {
    async fn find_lines_by_orders(
        &self,
        order_ids: &[PurchaseReturnOrderId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<PurchaseReturnLine>> {
        if order_ids.is_empty() {
            return Ok(Vec::new());
        }
        let order_ids: Vec<String> = order_ids.iter().map(ToString::to_string).collect();
        self.find_many(doc! { "purchase_return_order_id": { "$in": order_ids } }, executor).await
    }
}

/// 采购退货单列表投影字段。
///
/// # 返回
/// 返回投影条件文档。
fn purchase_return_order_projection() -> Document {
    doc! {
        "id": 1,
        "status": 1,
        "current_revision_id": 1,
        "created_by": 1,
        "updated_by": 1,
        "purchase_return_no": 1,
        "purchase_order_id": 1,
        "sales_return_case_id": 1,
        "return_mode": 1,
        "version": 1,
        "created_at": 1,
    }
}

#[cfg(test)]
mod tests {
    use mongodb::bson::doc;
    use persistence_core::QueryFilter;

    use super::PurchaseReturnOrderFilter;
    use crate::entity::returns::PurchaseReturnStatus;

    #[test]
    fn purchase_return_filter_filters_by_order_and_status() {
        let filter = PurchaseReturnOrderFilter {
            purchase_order_id: Some(erp_core::ids::PurchaseOrderId::new("po-1")),
            status: Some(PurchaseReturnStatus::Returned),
            ..Default::default()
        };

        let document = filter.to_doc();
        assert_eq!(document.get_str("purchase_order_id").unwrap(), "po-1");
        assert_eq!(document.get_str("status").unwrap(), "returned");
    }

    /// 授权空集与越界原单必须保持恒假条件，不得退化为无授权限制。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 无。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// `Some([])` 与筛选原单不在授权集时都必须写入 `$expr: false`。
    #[test]
    fn purchase_return_missing_authorized_source_ids_stay_empty() {
        use entity_core::NOT_DELETED_TIMESTAMP_BSON;
        let empty = PurchaseReturnOrderFilter {
            authorized_purchase_order_ids: Some(Vec::new()),
            ..Default::default()
        };
        assert_eq!(empty.to_doc(), doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON, "$expr": false });

        let out_of_scope = PurchaseReturnOrderFilter {
            purchase_order_id: Some(erp_core::ids::PurchaseOrderId::new("po-1")),
            authorized_purchase_order_ids: Some(vec!["po-2".into()]),
            ..Default::default()
        };
        assert_eq!(out_of_scope.to_doc(), doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON, "$expr": false });

        let in_scope = PurchaseReturnOrderFilter {
            purchase_order_id: Some(erp_core::ids::PurchaseOrderId::new("po-1")),
            authorized_purchase_order_ids: Some(vec!["po-1".into()]),
            ..Default::default()
        };
        assert_eq!(
            in_scope.to_doc(),
            doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON, "purchase_order_id": "po-1" }
        );
    }
}
