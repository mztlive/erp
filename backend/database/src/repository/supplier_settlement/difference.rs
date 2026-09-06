use crate::repository::owned::{
    SupplierSettlementDifferenceEvidenceRepository, SupplierSettlementDifferenceRepository,
};
use entities::supplier_settlement::{
    SettlementDifferenceStatus, SettlementDifferenceType, SupplierSettlementDifference,
    SupplierSettlementDifferenceEvidence,
};
use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use erp_core::common::time::Instant;
use erp_core::ids::SupplierSettlementItemId;
use mongodb::bson::{doc, Document};
use mongodb::options::FindOptions;
use serde::{Deserialize, Serialize};

use super::projection::{difference_sort_doc, supplier_settlement_difference_projection};
use persistence_core::Executor;
use persistence_core::{mongo_ops, Result};
use persistence_core::{PageResult, Pagination, QueryFilter};

/// 供应商结算差异列表投影行。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SupplierSettlementDifferenceRow {
    /// 实体主键。
    pub id: String,
    /// 所属结算明细。
    pub statement_item_id: SupplierSettlementItemId,
    /// 差异类型。
    pub difference_type: SettlementDifferenceType,
    /// 差异金额。
    pub difference_amount: erp_core::money::Amount,
    /// 差异状态。
    pub status: SettlementDifferenceStatus,
    /// 处理结果文本。
    pub resolution: Option<String>,
    /// 处理人。
    pub resolved_by: Option<String>,
    /// 处理时间。
    pub resolved_at: Option<Instant>,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 供应商结算差异列表筛选条件。
#[derive(Debug, Clone)]
pub struct SupplierSettlementDifferenceFilter {
    /// 所属结算明细；`None` 表示不筛选。
    pub statement_item_id: Option<SupplierSettlementItemId>,
    /// 差异状态；`None` 表示不筛选。
    pub status: Option<SettlementDifferenceStatus>,
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 排序字段（白名单内生效，白名单外回退 `created_at`）。
    pub sort_by: Option<String>,
    /// 是否升序；`false` 表示降序（默认）。
    pub sort_ascending: bool,
}

impl QueryFilter for SupplierSettlementDifferenceFilter {
    /// 转换为 MongoDB 查询条件（自动追加未删除过滤）。
    ///
    /// # 返回
    /// 返回查询条件文档。
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        if let Some(statement_item_id) = &self.statement_item_id {
            filter.insert("statement_item_id", statement_item_id.to_string());
        }
        if let Some(status) = self.status {
            filter.insert("status", status.as_str());
        }
        filter
    }
}

impl Pagination for SupplierSettlementDifferenceFilter {
    /// 返回页码与单页条数。
    ///
    /// # 返回
    /// 返回 `(page, page_size)` 元组。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

impl<'a> SupplierSettlementDifferenceEvidenceRepository<'a> {
    /// 按稳定请求 ID 查找不可变差异补证。
    pub async fn find_by_request_id(
        &self,
        request_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<SupplierSettlementDifferenceEvidence>> {
        self.find_one(doc! { "request_id": request_id }, executor).await
    }

    /// 批量读取差异对应的全部补证，避免详情 N+1。
    pub async fn find_by_difference_ids(
        &self,
        difference_ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<SupplierSettlementDifferenceEvidence>> {
        if difference_ids.is_empty() {
            return Ok(Vec::new());
        }
        self.find_many_sorted(
            doc! { "difference_id": { "$in": difference_ids } },
            doc! { "provided_at": 1, "id": 1 },
            executor,
        )
        .await
    }
}

impl<'a> SupplierSettlementDifferenceRepository<'a> {
    /// 按结算明细批量读取差异，按创建时间和主键升序排列。
    ///
    /// # 参数
    /// * `statement_item_ids` - 结算明细主键集合
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回关联这些结算明细的全部未删除差异。
    ///
    /// # 错误
    /// MongoDB 查询或游标读取失败时返回错误。
    pub async fn list_by_statement_item_ids(
        &self,
        statement_item_ids: &[SupplierSettlementItemId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<SupplierSettlementDifference>> {
        if statement_item_ids.is_empty() {
            return Ok(Vec::new());
        }
        self.find_many_sorted(
            doc! {
                "statement_item_id": {
                    "$in": statement_item_ids.iter().map(ToString::to_string).collect::<Vec<_>>()
                }
            },
            doc! { "created_at": 1, "id": 1 },
            executor,
        )
        .await
    }

    /// 分页检索供应商结算差异列表（投影查询）。
    ///
    /// 只返回 [`SupplierSettlementDifferenceRow`] 所需的列表字段，不加载整文档；
    /// 排序字段走白名单映射（`DIFFERENCE_SORT_FIELDS`），白名单外一律回退 `created_at`。
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
    pub async fn search_supplier_settlement_differences(
        &self,
        filter: &SupplierSettlementDifferenceFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<SupplierSettlementDifferenceRow>> {
        let options = FindOptions::builder()
            .sort(difference_sort_doc(
                filter.sort_by.as_deref(),
                filter.sort_ascending,
            ))
            .skip(filter.skip())
            .limit(filter.limit())
            .projection(supplier_settlement_difference_projection())
            .build();
        let collection = self
            .collection()
            .clone_with_type::<SupplierSettlementDifferenceRow>();
        let items = mongo_ops::find_many(&collection, filter.to_doc(), options, executor).await?;
        let total = mongo_ops::count_documents(&self.collection(), filter.to_doc(), executor).await?;

        Ok(PageResult {
            items,
            total: total as i64,
        })
    }
}
