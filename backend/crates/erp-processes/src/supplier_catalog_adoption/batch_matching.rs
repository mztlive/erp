//! 从同一冻结来源商品内的实际 SKU 匹配证明新增规格归属。

use std::collections::{HashMap, HashSet};

use application_core::AuditActor;
use erp_catalog::CatalogExt;
use erp_supply::catalog_adoption::{
    AdoptionMapping, AdoptionResult, AdoptionTarget, BatchAnchorTarget, BatchItemStatus, BatchMatchAnchor,
    CatalogAdoptionBatch, CatalogAdoptionBatchItem, has_product_receipt,
};
use erp_supply::entity::supplier_catalog::SupplierCatalogItem;
use persistence_core::Executor;

use super::batch_preview::batch_preview;
use super::confirm::AdoptionExecution;
use super::matching::{MatchKind, resolution_digest};
use super::{AdoptionResolution, SupplierCatalogAdoptionProcess};
use crate::{Error, Result};

impl SupplierCatalogAdoptionProcess {
    /// 从完整冻结范围提取真实已有 SKU 的公司商品依据，不接受外部目标参数。
    pub(super) async fn preview_group_anchor_candidates(
        &self,
        rows: &[CatalogAdoptionBatchItem],
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, BatchMatchAnchor>> {
        let (mut anchors, mut ambiguous) = (HashMap::<String, BatchMatchAnchor>::new(), HashSet::new());
        for row in rows {
            if !row.editable() || row.status == BatchItemStatus::Excluded {
                continue;
            }
            let resolution = match self.resolve_item(&row.source, &row.mapping, actor, executor).await {
                Ok(value) if matches!(value.kind, MatchKind::Existing) => value,
                Ok(_) => continue,
                Err(error) if row_matching_error(&error) => continue,
                Err(error) => return Err(error),
            };
            let target = self.group_anchor_target(&resolution, executor).await?;
            let key = row.source.product_id.clone();
            if anchors.get(&key).is_some_and(|existing| existing.target.product_id != target.product_id) {
                ambiguous.insert(key.clone());
            }
            anchors.entry(key).or_insert_with(|| BatchMatchAnchor {
                source: row.source.clone(),
                mapping: row.mapping.clone(),
                target,
            });
        }
        anchors.retain(|key, _| !ambiguous.contains(key));
        Ok(anchors)
    }

    /// 将经当前事务重验的同源依据传给正常自动匹配，不绕过身份或授权。
    pub(super) async fn resolve_batch_item(
        &self,
        item: &SupplierCatalogItem,
        mapping: &AdoptionMapping,
        anchor: Option<&BatchMatchAnchor>,
        command: Option<&str>,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<AdoptionResolution> {
        let Some(anchor) = anchor else { return self.resolve_item(item, mapping, actor, executor).await };
        self.revalidate_group_anchor(item, anchor, command, actor, executor).await?;
        let mut resolution = self
            .resolve_item_with_parent(item, mapping, Some(&anchor.target.product_id), actor, executor)
            .await?;
        resolution.group_anchor = Some(anchor.clone());
        let mapping = self.resolve_unit_mapping(item, mapping, actor, executor).await?;
        resolution.expected_match = resolution_digest(item, &mapping, &resolution)?;
        Ok(resolution)
    }

    async fn revalidate_group_anchor(
        &self,
        item: &SupplierCatalogItem,
        anchor: &BatchMatchAnchor,
        command: Option<&str>,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let current = self.load_item(&item.connection_id, &anchor.source.base.id, executor).await?;
        anchor.ensure_source(item, &current)?;
        let resolution = self.resolve_item(&current, &anchor.mapping, actor, executor).await?;
        let target = self.group_anchor_target(&resolution, executor).await?;
        let same_operation = match command {
            Some(command) => has_product_receipt(&self.db, command, &target.product_id, executor).await?,
            None => false,
        };
        anchor.ensure_target(&target, same_operation)?;
        Ok(())
    }

    async fn group_anchor_target(
        &self,
        resolution: &AdoptionResolution,
        executor: &mut dyn Executor,
    ) -> Result<BatchAnchorTarget> {
        let AdoptionTarget::Existing { sku_id, expected_sku_version, expected_product_version, .. } =
            &resolution.target
        else {
            return Err(Error::ConflictError("同源依据未匹配到现有公司 SKU，请重新核对".into()));
        };
        let product_id = resolution
            .product_id
            .as_deref()
            .ok_or_else(|| Error::Internal("已有 SKU 缺少商品身份".into()))?;
        let product = self
            .db
            .products()
            .find_by_id(product_id, executor)
            .await?
            .ok_or_else(|| Error::ConflictError("同源依据的公司商品已不存在".into()))?;
        let revision = self
            .db
            .catalog()
            .current_product_revision(&product, executor)
            .await?
            .ok_or_else(|| Error::ConflictError("同源依据的公司商品资料已不存在".into()))?;
        Ok(BatchAnchorTarget {
            product_id: product_id.into(),
            sku_id: sku_id.clone(),
            sku_version: *expected_sku_version,
            product_version: *expected_product_version,
            product_revision_id: revision.base.id,
        })
    }

    /// 执行已确认的批次行；依据规格即使未勾选执行也必须在真正写事务内重验。
    pub(super) async fn confirm_batch_row(
        &self,
        batch: &CatalogAdoptionBatch,
        item: &CatalogAdoptionBatchItem,
        actor: &AuditActor,
    ) -> Result<AdoptionResult> {
        let row =
            item.frozen_row.clone().ok_or_else(|| Error::Internal("原入库资料缺失，请人工核查".into()))?;
        let command =
            item.operation_id.clone().ok_or_else(|| Error::Internal("原入库操作缺失，请人工核查".into()))?;
        self.confirm_execution(
            AdoptionExecution {
                preview: batch_preview(batch, item),
                source: item.source.clone(),
                row,
                command,
                group_anchor: item.resolution.as_ref().and_then(|value| value.group_anchor.clone()),
            },
            actor,
        )
        .await
    }
}

fn row_matching_error(error: &Error) -> bool {
    matches!(
        error,
        Error::ValidationError(_)
            | Error::InvalidInput(_)
            | Error::ConflictError(_)
            | Error::BusinessLogicError(_)
            | Error::NotFound(_)
            | Error::Forbidden(_)
            | Error::Logic(_)
    )
}
