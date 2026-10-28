//! 服务器重读来源、自动匹配并产生权威逐项核对结果。

use std::collections::HashSet;

use application_core::AuditActor;
use entity_core::BaseModel;
use erp_catalog::CatalogExt;
use erp_core::common::time::Instant;
use erp_core::ids::{SupplierAccountId, SupplierOfferingId, SupplierOfferingRevisionId};
use erp_supply::catalog_adoption::{
    AdoptionRow, AdoptionSalesPrices, AdoptionTarget, BatchAdoptionExt, BatchItemStatus, BatchMatchAnchor,
    BatchResolution, BatchSalesValues, CatalogAdoptionBatch, CatalogAdoptionBatchItem,
    CatalogAdoptionPreview, mark_product_mapping_conflicts,
};
use erp_supply::entity::supplier_catalog::SupplierCatalogItem;
use erp_supply::entity::supplier_offering::SupplierOfferingRevision;
use erp_supply::repository::SupplierOfferingExt;
use persistence_core::{Executor, NoTransaction, Transactional};

use super::batch::batch_view;
use super::matching::MatchKind;
use super::request::{offering_request, product_request};
use super::{AdoptionResolution, BatchVersionRequest, BatchView, SupplierCatalogAdoptionProcess};
use crate::source_media::PreparedSourceMedia;
use crate::{Error, Result};

impl SupplierCatalogAdoptionProcess {
    /// 重验当前来源和公司目标并计算权威售价；逐项短事务保存后才发布预览。
    /// # 参数
    /// 来源连接、批次、页面已读版本、可选重新核对集合和当前认证用户。
    /// # 返回
    /// 带逐项匹配、售价、提示及新预览版本的批次。
    /// # 错误
    /// 批次越权、版本过期、范围无效或存储失败时拒绝；失败不发布部分预览。
    pub async fn preview_batch(
        &self,
        connection: &str,
        id: &str,
        request: BatchVersionRequest,
        actor: &AuditActor,
    ) -> Result<BatchView> {
        let (batch, token) = self.begin_batch_draft(connection, id, request.expected_version, actor).await?;
        let outcome = self.preview_batch_members(&batch, &token, request.item_ids.as_deref(), actor).await;
        if outcome.is_err() {
            self.release_batch_draft(connection, id, &token, actor).await?;
        }
        outcome
    }

    async fn preview_batch_members(
        &self,
        batch: &CatalogAdoptionBatch,
        token: &str,
        item_ids: Option<&[String]>,
        actor: &AuditActor,
    ) -> Result<BatchView> {
        let mut rows = self.all_batch_items(&batch.base.id, &mut NoTransaction).await?;
        let anchors = self.preview_group_anchor_candidates(&rows, actor, &mut NoTransaction).await?;
        let selection = item_ids.map(|ids| ids.iter().collect::<HashSet<_>>());
        if let Some(selected) = &selection {
            let ids = item_ids.expect("显式选择已由 Some 分支确认");
            if selected.is_empty()
                || selected.len() != ids.len()
                || selected.iter().any(|id| {
                    !rows.iter().any(|row| {
                        &row.source.base.id == *id
                            && row.editable()
                            && row.status != BatchItemStatus::Excluded
                    })
                })
            {
                return Err(Error::ValidationError("重新核对项必须为本批可编辑且不重复的来源规格".into()));
            }
        }
        for row in &mut rows {
            let recheck = selection.as_ref().is_none_or(|ids| ids.contains(&row.source.base.id));
            if row.editable() && row.status != BatchItemStatus::Excluded && recheck {
                *row = self
                    .preview_batch_member(batch, token, row, anchors.get(&row.source.product_id), actor)
                    .await?;
            }
        }
        self.persist_batch_conflicts(batch, token, &mut rows, actor).await?;
        self.ensure_batch_rows_access(actor, &rows, &mut NoTransaction).await?;
        self.publish_batch_preview(batch, token, rows, actor).await
    }

    async fn preview_batch_member(
        &self,
        batch: &CatalogAdoptionBatch,
        token: &str,
        row: &CatalogAdoptionBatchItem,
        anchor: Option<&BatchMatchAnchor>,
        actor: &AuditActor,
    ) -> Result<CatalogAdoptionBatchItem> {
        let (process, connection, id, token, actor, mut row, generation) = (
            self.clone(),
            batch.connection_id.clone(),
            batch.base.id.clone(),
            token.to_owned(),
            actor.clone(),
            row.clone(),
            batch.base.version,
        );
        let anchor = anchor.cloned();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let mut current =
                        process.check_batch_draft(&connection, &id, &token, &actor, executor).await?;
                    process.ensure_source(&process.batch_access(&connection, &actor, executor).await?)?;
                    if let Err(error) = process
                        .preview_batch_item(&current, &mut row, generation, anchor.as_ref(), &actor, executor)
                        .await
                    {
                        row.status = BatchItemStatus::Draft;
                        row.messages = vec![error.to_string()];
                        row.preview_version = None;
                        row.frozen_row = None;
                    }
                    current.draft_started_at = Some(Instant::now().unix_secs());
                    process.db.catalog_adoption_batches().update(&mut current, executor).await?;
                    process.db.catalog_adoption_batch_items().update(&mut row, executor).await?;
                    Ok(row)
                })
            })
            .await
    }

    async fn persist_batch_conflicts(
        &self,
        batch: &CatalogAdoptionBatch,
        token: &str,
        rows: &mut [CatalogAdoptionBatchItem],
        actor: &AuditActor,
    ) -> Result<()> {
        let before: Vec<_> =
            rows.iter().map(|row| (row.status, row.messages.clone(), row.preview_version)).collect();
        mark_product_mapping_conflicts(rows)?;
        for (row, previous) in rows.iter_mut().zip(before) {
            if (row.status, row.messages.clone(), row.preview_version) == previous {
                continue;
            }
            let (process, connection, id, token, actor, mut updated) = (
                self.clone(),
                batch.connection_id.clone(),
                batch.base.id.clone(),
                token.to_owned(),
                actor.clone(),
                row.clone(),
            );
            *row = self
                .db
                .client()
                .clone()
                .with_transaction(move |executor| {
                    Box::pin(async move {
                        let mut current =
                            process.check_batch_draft(&connection, &id, &token, &actor, executor).await?;
                        current.draft_started_at = Some(Instant::now().unix_secs());
                        process.db.catalog_adoption_batches().update(&mut current, executor).await?;
                        process.db.catalog_adoption_batch_items().update(&mut updated, executor).await?;
                        Ok::<_, Error>(updated)
                    })
                })
                .await?;
        }
        Ok(())
    }

    async fn publish_batch_preview(
        &self,
        batch: &CatalogAdoptionBatch,
        token: &str,
        rows: Vec<CatalogAdoptionBatchItem>,
        actor: &AuditActor,
    ) -> Result<BatchView> {
        let (process, connection, id, token, actor, generation) = (
            self.clone(),
            batch.connection_id.clone(),
            batch.base.id.clone(),
            token.to_owned(),
            actor.clone(),
            batch.base.version,
        );
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let mut current =
                        process.check_batch_draft(&connection, &id, &token, &actor, executor).await?;
                    current.preview_version = Some(generation);
                    current.step = "REVIEW".into();
                    current.draft_operation = None;
                    current.draft_started_at = None;
                    process.db.catalog_adoption_batches().update(&mut current, executor).await?;
                    Ok(batch_view(current, rows))
                })
            })
            .await
    }

    async fn preview_batch_item(
        &self,
        batch: &CatalogAdoptionBatch,
        row: &mut CatalogAdoptionBatchItem,
        version: u64,
        anchor: Option<&BatchMatchAnchor>,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let current = self.load_item(&batch.connection_id, &row.source.base.id, executor).await?;
        if current.supplier_id != batch.supplier_id {
            return Err(Error::ConflictError("来源商品归属已变化，请核对供应商连接".into()));
        }
        if current.offering_id.is_some() {
            row.source = current;
            row.status = BatchItemStatus::Skipped;
            row.messages = vec!["已完成入库，保留现有商品及供给".into()];
            return Ok(());
        }
        if batch_preview(batch, row).ensure_current_item(&current).is_err() {
            row.previous_source = Some(row.source.clone());
            if let Some(terms) = &mut row.terms {
                terms.dropship_supply_price_gross = current.purchase_price.clone().unwrap_or_default();
            }
            row.source = current;
            row.sales_prices = row.sales_prices.reset_rule_values();
            row.invalidate_preview()?;
            return Err(Error::ConflictError(
                "来源资料或接口采购报价已变化，请查看新旧信息后重新核对".into(),
            ));
        }
        row.source = current;
        row.apply_defaults(&batch.defaults, batch.fill_policy)?;
        let resolution =
            self.resolve_batch_item(&row.source, &row.mapping, anchor, None, actor, executor).await?;
        self.preview_batch_prices(batch, row, &resolution, executor).await?;
        self.freeze_batch_preview_row(row, resolution, version, executor).await
    }

    async fn preview_batch_prices(
        &self,
        batch: &CatalogAdoptionBatch,
        row: &mut CatalogAdoptionBatchItem,
        resolution: &AdoptionResolution,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let existing = matches!(resolution.kind, MatchKind::Existing);
        row.sales_prices = if let AdoptionTarget::Existing { sku_id, .. } = &resolution.target {
            self.existing_sales_prices(sku_id, executor).await?
        } else {
            let values = if row.sales_prices.has_preserved_values() {
                BatchSalesValues::initial(row.source.market_price)
            } else {
                row.sales_prices.clone()
            };
            values.apply(&batch.rules, batch.fill_policy, row.source.purchase_quote().ok(), false)?
        };
        let warnings = row.sales_prices.validate(row.source.purchase_quote().ok(), existing)?;
        let messages: Vec<_> = warnings.iter().map(|warning| warning.label().to_owned()).collect();
        if row.warnings != messages {
            row.warnings_confirmed = false;
        }
        row.warnings = messages;
        Ok(())
    }

    async fn freeze_batch_preview_row(
        &self,
        row: &mut CatalogAdoptionBatchItem,
        resolution: AdoptionResolution,
        version: u64,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let terms =
            row.terms.clone().ok_or_else(|| Error::ValidationError("请补齐进项税率与供给有效期".into()))?;
        let frozen = AdoptionRow {
            item_id: row.source.base.id.clone(),
            target: AdoptionTarget::Auto {
                mapping: row.mapping.clone(),
                expected_match: resolution.expected_match.clone(),
            },
            terms,
            sales_prices: Some(row.sales_prices.to_sales_prices()),
            delivery_template_id: row.delivery_template_id.clone(),
        };
        self.validate_batch_offering(&row.source, &frozen, executor).await?;
        if !matches!(resolution.kind, MatchKind::Existing) {
            let mut resolved = frozen.clone();
            resolved.target = resolution.target;
            product_request(&row.source, &resolved, &PreparedSourceMedia::empty())?;
        }
        row.resolution = Some(BatchResolution {
            kind: match resolution.kind {
                MatchKind::Existing => "EXISTING",
                MatchKind::CreateSku => "CREATE_SKU",
                MatchKind::CreateProduct => "CREATE_PRODUCT",
            }
            .into(),
            expected_match: resolution.expected_match,
            sku_no: resolution.sku_no,
            product_id: resolution.product_id,
            reason: resolution.reason,
            group_anchor: resolution.group_anchor,
        });
        row.frozen_row = Some(frozen);
        row.preview_version = Some(version);
        row.status = BatchItemStatus::Ready;
        row.messages.clear();
        Ok(())
    }

    async fn existing_sales_prices(
        &self,
        sku_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<BatchSalesValues> {
        let sku = self
            .db
            .skus()
            .find_by_id(sku_id, executor)
            .await?
            .ok_or_else(|| Error::ConflictError("公司规格已不存在，请重新核对".into()))?;
        let revision = self
            .db
            .catalog()
            .current_sku_revision(&sku, executor)
            .await?
            .ok_or_else(|| Error::ConflictError("公司规格缺少当前价格资料".into()))?;
        Ok(BatchSalesValues::existing(AdoptionSalesPrices {
            sales_visible_price_gross: revision.sales_visible_price_gross,
            bulk_price_gross: revision.bulk_price_gross,
            bulk_min_quantity: revision.bulk_min_quantity,
            market_price: revision.market_price,
        }))
    }

    /// 核验只读采购报价、供给条款与配送模板归属，不创建业务事实。
    /// # 参数
    /// `item` 为已核验来源，`row` 为服务端冻结载荷，`executor` 为当前短事务执行器。
    /// # 返回
    /// 条款与模板均满足现行供给规则时成功。
    /// # 错误
    /// 报价不符、条款无效、模板缺失或跨供应商引用时拒绝。
    pub(super) async fn validate_batch_offering(
        &self,
        item: &SupplierCatalogItem,
        row: &AdoptionRow,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let request = offering_request(item, row, "batch-preview", "batch-preview")?;
        SupplierOfferingRevision::new(
            SupplierOfferingRevisionId::new("batch-preview"),
            request.try_into_initial_revision(SupplierOfferingId::new("batch-preview"))?,
        )?;
        if let Some(id) = &row.delivery_template_id {
            let template = self
                .db
                .supplier_delivery_templates()
                .find_by_id(id, executor)
                .await?
                .ok_or_else(|| Error::ValidationError("配送模板不存在，请重新选择".into()))?;
            if !template.belongs_to(&SupplierAccountId::new(item.supplier_id.clone())) {
                return Err(Error::ValidationError("配送模板不属于当前供应商".into()));
            }
        }
        Ok(())
    }
}

/// 将批次行还原为现行逐项确认所需的冻结来源视图。
/// # 参数
/// `batch` 为已授权批次，`row` 为保留原确认人和来源内容的批次行。
/// # 返回
/// 使用原确认人及固定来源的内部预览，不改变浏览器确认范围。
/// # 错误
/// 不返回错误；原操作载荷与来源版本由实际确认流程重新检查。
pub(super) fn batch_preview(
    batch: &CatalogAdoptionBatch,
    row: &CatalogAdoptionBatchItem,
) -> CatalogAdoptionPreview {
    CatalogAdoptionPreview {
        base: BaseModel::new(batch.base.id.clone()),
        connection_id: batch.connection_id.clone(),
        actor_id: row.submitted_by.clone().unwrap_or_else(|| batch.actor_id.clone()),
        // 批次资格由持久化预览版本控制，不使用旧 HTTP 预览的时间有效期。
        expires_at: Instant::now(),
        items: vec![row.source.clone()],
    }
}
