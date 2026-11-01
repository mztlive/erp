//! 冻结选择范围和持久化批次读取。

use std::collections::HashSet;

use application_core::AuditActor;
use entity_core::BaseModel;
use erp_supply::catalog_adoption::{
    AdoptionMapping, BatchAdoptionExt, BatchCounts, BatchHistoryEntry, BatchItemStatus, BatchSalesValues,
    CatalogAdoptionBatch, CatalogAdoptionBatchItem, ValueOrigin, batch_history, batch_items,
};
use erp_supply::entity::supplier_api::SupplierApiConnection;
use erp_supply::entity::supplier_catalog::SupplierCatalogItem;
use erp_supply::service::supplier_api::SupplierApiService;
use id_generator::next_id;
use persistence_core::{Executor, NoTransaction};

use super::{BatchView, SupplierCatalogAdoptionProcess};
use crate::{Error, Result};

impl SupplierCatalogAdoptionProcess {
    /// 读取完整批次；每次重验当前权限及供应商数据范围。
    /// # 参数
    /// 来源连接、批次身份和当前认证用户。
    /// # 返回
    /// 已保存草稿、最后预览和真实后台结果。
    /// # 错误
    /// 越权、批次不存在或查询失败时拒绝。
    pub async fn batch(&self, connection: &str, id: &str, actor: &AuditActor) -> Result<BatchView> {
        let batch = self.preparation_batch(connection, id, actor, &mut NoTransaction).await?;
        if !batch.selection_complete || batch.work.as_ref().is_some_and(|work| work.blocking()) {
            return Ok(batch_summary(batch));
        }
        let rows = self.all_batch_items(id, &mut NoTransaction).await?;
        self.ensure_batch_rows_access(actor, &rows, &mut NoTransaction).await?;
        Ok(batch_view(batch, rows))
    }

    /// 列出当前供应商可访问的最近批次，以便离开页面后重新打开。
    /// # 参数
    /// 来源连接和当前用户。
    /// # 返回
    /// 最多二十个最近批次及当前成员状态摘要，受当前供应商范围约束。
    /// # 错误
    /// 权限或数据范围不足及查询失败时拒绝。
    pub async fn batches(&self, connection: &str, actor: &AuditActor) -> Result<Vec<BatchHistoryEntry>> {
        self.batch_access(connection, actor, &mut NoTransaction).await?;
        Ok(batch_history(&self.db, connection, &mut NoTransaction).await?)
    }

    pub(super) async fn load_batch(
        &self,
        connection: &str,
        id: &str,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<CatalogAdoptionBatch> {
        let source = self.batch_access(connection, actor, executor).await?;
        self.db
            .catalog_adoption_batches()
            .find_by_id(id, executor)
            .await?
            .filter(|batch| {
                batch.selection_complete
                    && batch.connection_id == connection
                    && batch.supplier_id == source.supplier_id.as_ref()
            })
            .ok_or_else(|| Error::NotFound("入库批次不存在或不在当前数据范围".into()))
    }

    pub(super) async fn batch_access(
        &self,
        connection: &str,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<SupplierApiConnection> {
        self.ensure_permission(actor, "supplier_api_connection:catalog_sync").await?;
        self.ensure_permission(actor, "supplier_offering_cost:detail").await?;
        let source = SupplierApiService::new(self.db.clone()).load_connection(connection, executor).await?;
        self.services.suppliers.require_with(actor, "detail", source.supplier_id.as_ref(), executor).await?;
        Ok(source)
    }

    pub(super) async fn ensure_batch_rows_access(
        &self,
        actor: &AuditActor,
        rows: &[CatalogAdoptionBatchItem],
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let mut products = HashSet::new();
        let mut offerings = HashSet::new();
        for row in rows {
            if let Some(result) = &row.result {
                products.insert(result.product_id.as_str());
                offerings.insert(result.offering_id.as_str());
            }
            if let Some(product) = row.resolution.as_ref().and_then(|value| value.product_id.as_deref()) {
                products.insert(product);
            }
            if let Some(offering) = row.source.offering_id.as_deref() {
                offerings.insert(offering);
            }
            if let Some(product) = row.source.company_product_id.as_deref() {
                products.insert(product);
            }
        }
        for product in products {
            self.services.catalog_access.require_product(actor, "detail", product, executor).await?;
        }
        for offering in offerings {
            self.services.offering_access.require_offering(actor, "list", offering, executor).await?;
        }
        Ok(())
    }

    pub(super) async fn all_batch_items(
        &self,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Vec<CatalogAdoptionBatchItem>> {
        let (mut rows, mut after) = (Vec::new(), String::new());
        loop {
            let page = batch_items(&self.db, id, &after, 500, executor).await?;
            let Some(last) = page.last() else { break };
            after = last.base.id.clone();
            rows.extend(page);
        }
        Ok(rows)
    }
}

/// 根据当前来源资料初始化一项草稿，不产生商品或供给事实。
/// # 参数
/// 已登记批次与属于其固定成员的来源资料。
/// # 返回
/// 尚未核对的草稿，保留已采纳及排除状态。
/// # 错误
/// 不返回错误。
pub(super) fn new_batch_item(
    batch: &CatalogAdoptionBatch,
    source: SupplierCatalogItem,
) -> CatalogAdoptionBatchItem {
    let excluded = batch.excluded_ids.contains(&source.base.id);
    let initially_adopted = source.offering_id.is_some();
    CatalogAdoptionBatchItem {
        base: BaseModel::new(next_id()),
        batch_id: batch.base.id.clone(),
        sales_prices: BatchSalesValues::initial(source.market_price),
        mapping: AdoptionMapping {
            category_id: String::new(),
            unit_id: String::new(),
            product_kind: "PHYSICAL".into(),
            company_name: None,
            company_description: None,
        },
        source,
        previous_source: None,
        initially_adopted,
        terms: None,
        delivery_template_id: None,
        field_origins: Default::default(),
        status: if excluded {
            BatchItemStatus::Excluded
        } else if initially_adopted {
            BatchItemStatus::Skipped
        } else {
            BatchItemStatus::Draft
        },
        messages: vec![],
        warnings: vec![],
        warnings_confirmed: false,
        resolution: None,
        frozen_row: None,
        result: None,
        operation_id: None,
        processing_started_at: None,
        preview_version: None,
        submitted_by: None,
    }
}

pub(super) fn batch_view(batch: CatalogAdoptionBatch, rows: Vec<CatalogAdoptionBatchItem>) -> BatchView {
    let product_count = rows
        .iter()
        .filter(|row| row.status != BatchItemStatus::Excluded)
        .map(|row| &row.source.product_id)
        .collect::<HashSet<_>>()
        .len();
    let manual_count = rows
        .iter()
        .filter(|row| {
            [
                row.sales_prices.dropship.origin,
                row.sales_prices.bulk.origin,
                row.sales_prices.minimum_quantity.origin,
                row.sales_prices.market_price.origin,
            ]
            .contains(&ValueOrigin::Manual)
                || row.field_origins.values().any(|origin| *origin == ValueOrigin::Manual)
        })
        .count();
    BatchView {
        rows_complete: true,
        counts: BatchCounts::from_items(&rows),
        batch,
        rows,
        product_count,
        manual_count,
    }
}

/// 返回只含批次头的受理/进度回执，客户端须保留已有明细或等待完成后读取。
/// # 参数
/// 已授权批次。
/// # 返回
/// 明确标注明细不完整的响应。
/// # 错误
/// 不返回错误。
pub(super) fn batch_summary(batch: CatalogAdoptionBatch) -> BatchView {
    let mut view = batch_view(batch, vec![]);
    view.rows_complete = false;
    view
}
