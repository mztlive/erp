use crate::entity::supplier_settlement::SupplierSettlementItem;
use crate::repository::owned::SupplierSettlementItemRepository;
use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use mongodb::bson::{doc, Document};
use mongodb::options::FindOptions;
use serde::{Deserialize, Serialize};

use super::projection::{item_sort_doc, supplier_settlement_item_projection};
use persistence_core::Executor;
use persistence_core::{mongo_ops, Result};
use persistence_core::{PageResult, Pagination, QueryFilter};

/// 供应商结算明细列表投影行。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SupplierSettlementItemRow {
    /// 实体主键。
    pub id: String,
    /// 所属结算单。
    pub statement_id: erp_core::ids::SupplierSettlementStatementId,
    /// 供应商子订单。
    pub supplier_fulfillment_order_id: erp_core::ids::SupplierFulfillmentOrderId,
    /// 供应商履约明细。
    pub supplier_fulfillment_item_id: erp_core::ids::SupplierFulfillmentItemId,
    /// 来源快照冻结数量。
    pub quantity: erp_core::money::Quantity,
    /// 订单结算金额。
    pub order_amount: erp_core::money::Amount,
    /// 运费金额。
    pub freight_amount: erp_core::money::Amount,
    /// 服务费金额。
    pub service_fee_amount: erp_core::money::Amount,
    /// 供应商退款金额。
    pub refund_amount: erp_core::money::Amount,
    /// ERP 计算含税金额。
    pub erp_calculated_amount: erp_core::money::Amount,
    /// ERP 计算不含税金额。
    pub erp_calculated_net_amount: erp_core::money::Amount,
    /// ERP 计算税额。
    pub erp_calculated_tax_amount: erp_core::money::Amount,
    /// 供应商账单含税金额。
    pub supplier_billed_amount: erp_core::money::Amount,
    /// 供应商账单不含税金额。
    pub supplier_billed_net_amount: erp_core::money::Amount,
    /// 供应商账单税额。
    pub supplier_billed_tax_amount: erp_core::money::Amount,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 供应商结算明细列表筛选条件。
#[derive(Debug, Clone)]
pub struct SupplierSettlementItemFilter {
    /// 所属结算单；`None` 表示不筛选。
    pub statement_id: Option<erp_core::ids::SupplierSettlementStatementId>,
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 排序字段（白名单内生效，白名单外回退 `created_at`）。
    pub sort_by: Option<String>,
    /// 是否升序；`false` 表示降序（默认）。
    pub sort_ascending: bool,
}

impl QueryFilter for SupplierSettlementItemFilter {
    /// 转换为 MongoDB 查询条件（自动追加未删除过滤）。
    ///
    /// # 返回
    /// 返回查询条件文档。
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        if let Some(statement_id) = &self.statement_id {
            filter.insert("statement_id", statement_id.to_string());
        }
        filter
    }
}

impl Pagination for SupplierSettlementItemFilter {
    /// 返回页码与单页条数。
    ///
    /// # 返回
    /// 返回 `(page, page_size)` 元组。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

impl<'a> SupplierSettlementItemRepository<'a> {
    /// 按结算单读取全部冻结明细，按创建时间和主键升序排列。
    ///
    /// # 参数
    /// * `statement_id` - 供应商结算单主键
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回该结算单的全部未删除冻结明细。
    ///
    /// # 错误
    /// MongoDB 查询或游标读取失败时返回错误。
    pub async fn list_by_statement(
        &self,
        statement_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Vec<SupplierSettlementItem>> {
        self.find_many_sorted(
            doc! { "statement_id": statement_id },
            doc! { "created_at": 1, "id": 1 },
            executor,
        )
        .await
    }

    /// 按结算单主键批量读取全部冻结明细。
    ///
    /// # 参数
    /// * `statement_ids` - 已授权结算单主键集合
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回按结算单、创建时间和主键稳定排序的冻结明细。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    pub async fn list_by_statement_ids(
        &self,
        statement_ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<SupplierSettlementItem>> {
        if statement_ids.is_empty() {
            return Ok(Vec::new());
        }
        self.find_many_sorted(
            doc! { "statement_id": { "$in": statement_ids } },
            doc! { "statement_id": 1, "created_at": 1, "id": 1 },
            executor,
        )
        .await
    }

    /// 分页检索供应商结算明细列表（投影查询）。
    ///
    /// 只返回 [`SupplierSettlementItemRow`] 所需的列表字段，不加载整文档；
    /// 排序字段走白名单映射（`ITEM_SORT_FIELDS`），白名单外一律回退 `created_at`。
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
    pub async fn search_supplier_settlement_items(
        &self,
        filter: &SupplierSettlementItemFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<SupplierSettlementItemRow>> {
        let options = FindOptions::builder()
            .sort(item_sort_doc(filter.sort_by.as_deref(), filter.sort_ascending))
            .skip(filter.skip())
            .limit(filter.limit())
            .projection(supplier_settlement_item_projection())
            .build();
        let collection = self.collection().clone_with_type::<SupplierSettlementItemRow>();
        let items = mongo_ops::find_many(&collection, filter.to_doc(), options, executor).await?;
        let total = mongo_ops::count_documents(&self.collection(), filter.to_doc(), executor).await?;

        Ok(PageResult {
            items,
            total: total as i64,
        })
    }
}
