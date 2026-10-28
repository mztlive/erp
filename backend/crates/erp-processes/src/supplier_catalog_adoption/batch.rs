//! 冻结选择范围和持久化批次读取。

use std::collections::HashSet;

use application_core::AuditActor;
use entity_core::BaseModel;
use erp_core::common::time::BusinessDate;
use erp_supply::catalog_adoption::{
    AdoptionMapping, BatchAdoptionExt, BatchCounts, BatchDefaults, BatchItemStatus, BatchPricing,
    BatchSalesValues, CatalogAdoptionBatch, CatalogAdoptionBatchItem, FillPolicy, ValueOrigin, batch_items,
    recent_batches,
};
use erp_supply::entity::supplier_api::SupplierApiConnection;
use erp_supply::entity::supplier_catalog::SupplierCatalogItem;
use erp_supply::repository::supplier_catalog;
use erp_supply::service::supplier_api::SupplierApiService;
use id_generator::{DocumentNumberGenerator, DocumentNumberKind, next_id};
use persistence_core::{Executor, NoTransaction, Transactional};

use super::{BatchView, CreateBatchRequest, SupplierCatalogAdoptionProcess};
use crate::adapters::{catalog_access, offering_access, supplier_access};
use crate::{Error, Result};

impl SupplierCatalogAdoptionProcess {
    /// 冻结整个选择集合并创建可恢复草稿，之后新同步记录不加入本批。
    /// # 参数
    /// `connection` 为来源连接，`request` 指定明确集合或筛选，`actor` 为当前用户。
    /// # 返回
    /// 服务端持久化的批次与完整成员。
    /// # 错误
    /// 越权、来源不可用、选择无效或存储失败时拒绝。
    pub async fn create_batch(
        &self,
        connection: &str,
        request: CreateBatchRequest,
        actor: &AuditActor,
    ) -> Result<BatchView> {
        self.batch_access(connection, actor, &mut NoTransaction).await?;
        let batch_no = DocumentNumberGenerator::new(self.db.clone())
            .next_number(
                DocumentNumberKind::SupplierCatalogAdoption,
                BusinessDate::today().as_naive_date(),
                &mut NoTransaction,
            )
            .await
            .map_err(|error| Error::Internal(error.to_string()))?;
        let (process, source_id, acting) = (self.clone(), connection.to_owned(), actor.clone());
        let selected = request.clone();
        let (source, items) = self
            .db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let source = process.batch_access(&source_id, &acting, executor).await?;
                    process.ensure_source(&source)?;
                    let items = process.batch_selection(&source_id, &selected, executor).await?;
                    Ok::<_, Error>((source, items))
                })
            })
            .await?;
        let id = self.persist_batch_selection(&source, items, request, batch_no, actor).await?;
        self.batch(connection, &id, actor).await
    }

    async fn persist_batch_selection(
        &self,
        source: &SupplierApiConnection,
        items: Vec<SupplierCatalogItem>,
        request: CreateBatchRequest,
        batch_no: String,
        actor: &AuditActor,
    ) -> Result<String> {
        let ids: HashSet<_> = items.iter().map(|item| item.base.id.as_str()).collect();
        if request.excluded_ids.iter().collect::<HashSet<_>>().len() != request.excluded_ids.len()
            || request.excluded_ids.iter().any(|id| !ids.contains(id.as_str()))
        {
            return Err(Error::ValidationError("排除项不属于本次冻结范围".into()));
        }
        let mut batch = CatalogAdoptionBatch {
            base: BaseModel::new(next_id()),
            batch_no,
            connection_id: source.base.id.clone(),
            supplier_id: source.supplier_id.to_string(),
            actor_id: actor.id().into(),
            selection_filter: request.filter,
            selected_count: items.len(),
            excluded_ids: request.excluded_ids,
            rules: BatchPricing::default(),
            fill_policy: FillPolicy::default(),
            defaults: BatchDefaults::default(),
            step: "PRICING".into(),
            preview_version: None,
            submitted: false,
            selection_complete: false,
            execution_pending: false,
            draft_operation: None,
            draft_started_at: None,
            inactive_pricing: None,
            published_operations: vec![],
            confirmations: vec![],
        };
        self.db.catalog_adoption_batches().create(&batch, &mut NoTransaction).await?;
        for item in items {
            source.ensure_offering_source(&item.supplier_id)?;
            let row = new_batch_item(&batch, item);
            self.db.catalog_adoption_batch_items().create(&row, &mut NoTransaction).await?;
        }
        self.batch_access(&source.base.id, actor, &mut NoTransaction).await?;
        batch.selection_complete = true;
        self.db.catalog_adoption_batches().update(&mut batch, &mut NoTransaction).await?;
        Ok(batch.base.id)
    }

    async fn batch_selection(
        &self,
        connection: &str,
        request: &CreateBatchRequest,
        executor: &mut dyn Executor,
    ) -> Result<Vec<SupplierCatalogItem>> {
        match (&request.item_ids, &request.filter) {
            (Some(ids), None) => {
                let unique: HashSet<_> = ids.iter().collect();
                if ids.is_empty() || unique.len() != ids.len() || ids.iter().any(|id| id.trim().is_empty()) {
                    return Err(Error::ValidationError("请选择不重复的来源规格".into()));
                }
                let mut rows = Vec::with_capacity(ids.len());
                for id in ids {
                    rows.push(self.load_item(connection, id, executor).await?);
                }
                Ok(rows)
            },
            (None, Some(filter)) => {
                filter.validate()?;
                let (mut rows, mut after) = (Vec::new(), String::new());
                loop {
                    let page = supplier_catalog::filtered_after(
                        &self.db,
                        connection,
                        filter,
                        if after.is_empty() { None } else { Some(after.as_str()) },
                        500,
                        executor,
                    )
                    .await?;
                    let Some(last) = page.last() else { break };
                    after = last.base.id.clone();
                    rows.extend(page);
                }
                if rows.is_empty() {
                    return Err(Error::ValidationError("当前筛选没有可选商品".into()));
                }
                Ok(rows)
            },
            _ => Err(Error::ValidationError("勾选商品与全部筛选结果必须选择且只能选择一种".into())),
        }
    }

    /// 读取完整批次；每次重验当前权限及供应商数据范围。
    /// # 参数
    /// 来源连接、批次身份和当前认证用户。
    /// # 返回
    /// 已保存草稿、最后预览和真实后台结果。
    /// # 错误
    /// 越权、批次不存在或查询失败时拒绝。
    pub async fn batch(&self, connection: &str, id: &str, actor: &AuditActor) -> Result<BatchView> {
        let batch = self.load_batch(connection, id, actor, &mut NoTransaction).await?;
        let rows = self.all_batch_items(id, &mut NoTransaction).await?;
        self.ensure_batch_rows_access(actor, &rows, &mut NoTransaction).await?;
        Ok(batch_view(batch, rows))
    }

    /// 列出当前供应商可访问的最近批次，以便离开页面后重新打开。
    /// # 参数
    /// 来源连接和当前用户。
    /// # 返回
    /// 最多二十个最近批次，仍逐个受当前供应商范围约束。
    /// # 错误
    /// 权限或数据范围不足及查询失败时拒绝。
    pub async fn batches(&self, connection: &str, actor: &AuditActor) -> Result<Vec<CatalogAdoptionBatch>> {
        self.batch_access(connection, actor, &mut NoTransaction).await?;
        Ok(recent_batches(&self.db, connection, None, &mut NoTransaction).await?)
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
        supplier_access(self.db.clone(), self.rbac.clone())
            .require_with(actor, "detail", source.supplier_id.as_ref(), executor)
            .await?;
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
            catalog_access(self.db.clone(), self.rbac.clone())
                .require_product(actor, "detail", product, executor)
                .await?;
        }
        for offering in offerings {
            offering_access(self.db.clone(), self.rbac.clone())
                .require_offering(actor, "list", offering, executor)
                .await?;
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

fn new_batch_item(batch: &CatalogAdoptionBatch, source: SupplierCatalogItem) -> CatalogAdoptionBatchItem {
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
    BatchView { counts: BatchCounts::from_items(&rows), batch, rows, product_count, manual_count }
}
