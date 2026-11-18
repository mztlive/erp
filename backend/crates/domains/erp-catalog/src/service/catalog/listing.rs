//! SKU 上下架与 SPU 继承状态编排。
//!
//! `product` 不重复持久化上架字段；SPU 状态始终从当前启用 SKU 汇总。
//! 整组切换在一个事务内更新全部目标 SKU 并写一条审计日志。

use std::collections::HashMap;

use application_core::AuditActor;
use erp_core::ids::ProductId;
use erp_permissions::catalog as catalog_permissions;
use persistence_core::{Executor, Transactional};
use validator::Validate;

use super::CatalogService;
use super::support::ensure_version;
use crate::dto::{
    ProductListingView, ProductView, SkuView, UpdateProductListingRequest, UpdateSkuListingRequest,
};
use crate::entity::catalog::{ListingStatus, Product, ProductListingStatus, Sku};
use crate::error::{Error, Result};
use crate::repository::prelude::*;
use crate::repository::{CatalogExt, ProductListingSummary};
use crate::source_sync::repository::ProductSourceExt;

/// 整组上/下架在原授权事务内完成校验后形成的待写集合。
struct ProductListingChange {
    product_id: ProductId,
    changed: Vec<Sku>,
    view: ProductListingView,
}

impl CatalogService {
    /// 一次切换 SPU 下全部当前启用 SKU 的上架状态。
    ///
    /// 下架操作同时清理历史停用 SKU 的遗留上架状态；上架操作只作用于当前
    /// 启用 SKU。全部写入与审计日志在同一事务内提交。
    ///
    /// # 参数
    /// * `product_id` - 商品稳定 ID
    /// * `req` - 全部当前启用 SKU 的目标上架状态
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回 SPU 继承上架状态与 SKU 数量汇总。
    ///
    /// # 错误
    /// 商品不存在、停用商品尝试上架、没有可上架 SKU 或事务失败时返回错误。
    pub async fn product_listing_update(
        &self,
        product_id: &str,
        req: UpdateProductListingRequest,
        actor: &AuditActor,
    ) -> Result<ProductListingView> {
        let service = self.clone();
        let actor = actor.clone();
        let product_id = product_id.to_owned();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    service.product_listing_update_stage(&product_id, req, &actor, executor).await
                })
            })
            .await
    }

    /// 在原事务完成授权、事实读取、写入与回包。
    async fn product_listing_update_stage(
        &self,
        product_id: &str,
        req: UpdateProductListingRequest,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<ProductListingView> {
        let change =
            self.prepare_product_listing_change(product_id, req.listing_status, actor, executor).await?;
        if change.changed.is_empty() {
            return Ok(change.view);
        }
        self.write_product_listing_change(change, req.listing_status, actor, executor).await
    }

    /// 切换单个 SKU 的上架状态。
    ///
    /// # 参数
    /// * `sku_id` - SKU 稳定 ID
    /// * `req` - 期望版本与目标上架状态
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回更新后的 SKU 视图。
    ///
    /// # 错误
    /// SKU/商品不存在、版本冲突、停用对象尝试上架或事务失败时返回错误。
    pub async fn sku_listing_update(
        &self,
        sku_id: &str,
        req: UpdateSkuListingRequest,
        actor: &AuditActor,
    ) -> Result<SkuView> {
        let service = self.clone();
        let actor = actor.clone();
        let sku_id = sku_id.to_owned();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(
                    async move { service.sku_listing_update_stage(&sku_id, req, &actor, executor).await },
                )
            })
            .await
    }

    /// 在原事务完成授权、事实读取、写入与回包。
    async fn sku_listing_update_stage(
        &self,
        sku_id: &str,
        req: UpdateSkuListingRequest,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<SkuView> {
        req.validate()?;
        let (sku, changed) =
            self.prepare_sku_listing_change(sku_id, req.version, req.listing_status, actor, executor).await?;
        if !changed {
            return Ok(sku.into());
        }
        let sku = self.write_sku_listing_change(sku, req.listing_status, actor, executor).await?;
        Ok(sku.into())
    }

    /// 校验整组操作并构造需要持久化的 SKU 集合。
    async fn prepare_product_listing_change(
        &self,
        product_id: &str,
        target: ListingStatus,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<ProductListingChange> {
        let product = self
            .access()
            .require_product(actor, &catalog_permissions::PRODUCT_PRODUCT_UPDATE, product_id, executor)
            .await?;
        product.ensure_has_responsibility()?;
        if target.is_listed() && !product.is_active() {
            return Err(Error::BusinessLogicError("停用的商品不能上架".to_string()));
        }
        let product_id = ProductId::new(product.base.id);
        let skus = self.db.skus().find_by_product_ids(std::slice::from_ref(&product_id), executor).await?;
        let active_count = skus.iter().filter(|sku| sku.is_active()).count();
        if target.is_listed() && active_count == 0 {
            return Err(Error::BusinessLogicError("商品下没有可上架的 SKU".to_string()));
        }
        let changed = changed_skus(skus, target, actor.id())?;
        let view = product_listing_view(product_id.as_ref(), active_count, target);
        Ok(ProductListingChange { product_id, changed, view })
    }

    /// 沿用本事务准备阶段的完整授权，写入整组 SKU 状态与审计日志。
    async fn write_product_listing_change(
        &self,
        change: ProductListingChange,
        target: ListingStatus,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<ProductListingView> {
        let ProductListingChange { product_id, mut changed, view } = change;
        let audit = self.audit.resource_log_with_message(
            actor.clone(),
            "product.listing.update",
            "product",
            product_id.to_string(),
            Some(format!("整组{}", target.label())),
        )?;

        for sku in &mut changed {
            self.db.skus().update(sku, executor).await?;
        }
        self.audit.persist(&audit, executor).await?;

        Ok(view)
    }

    /// 校验单 SKU 操作并应用领域状态迁移。
    async fn prepare_sku_listing_change(
        &self,
        sku_id: &str,
        version: u64,
        target: ListingStatus,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<(Sku, bool)> {
        self.access().ensure_writable(actor, &catalog_permissions::PRODUCT_PRODUCT_UPDATE, executor).await?;
        let Some(mut sku) = self.db.skus().find_by_id(sku_id, executor).await? else {
            return Err(Error::NotFound("SKU 不存在".to_string()));
        };
        let product = self.authorize_sku_listing_change(&sku, version, actor, executor).await?;
        product.ensure_has_responsibility()?;
        if target.is_listed() && !product.is_active() {
            return Err(Error::BusinessLogicError("停用商品下的 SKU 不能上架".to_string()));
        }
        let changed = sku.set_listing_status(target, actor.id())?;
        Ok((sku, changed))
    }

    /// 先检查所属商品的完整修改资格，再报告当前 SKU 的版本冲突。
    async fn authorize_sku_listing_change(
        &self,
        sku: &Sku,
        version: u64,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<Product> {
        let product = self
            .access()
            .require_product(
                actor,
                &catalog_permissions::PRODUCT_PRODUCT_UPDATE,
                sku.product_id.as_ref(),
                executor,
            )
            .await?;
        ensure_version(sku.base.version, version)?;
        Ok(product)
    }

    /// 沿用本事务准备阶段的完整授权，写入单 SKU 状态与审计日志。
    async fn write_sku_listing_change(
        &self,
        mut sku: Sku,
        target: ListingStatus,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<Sku> {
        let audit = self.audit.resource_log_with_message(
            actor.clone(),
            "sku.listing.update",
            "sku",
            sku.base.id.clone(),
            Some(target.label().to_string()),
        )?;

        self.db.skus().update(&mut sku, executor).await?;
        self.audit.persist(&audit, executor).await?;
        Ok(sku)
    }

    /// 为一组 SPU 批量计算继承上架状态，避免列表 N+1。
    ///
    /// # 参数
    /// * `executor` - 原授权事务或读取阶段的执行器。
    /// * `product_ids` - 要汇总的商品稳定 ID
    ///
    /// # 返回
    /// 返回按商品 ID 索引的继承上架视图。
    ///
    /// # 错误
    /// 上架汇总读取失败时返回仓储错误。
    pub(super) async fn product_listing_views(
        &self,
        product_ids: &[ProductId],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, ProductListingView>> {
        let summaries = self.db.catalog().listing_summaries(product_ids, executor).await?;
        Ok(product_listing_views_from_summaries(product_ids, &summaries))
    }

    /// 将商品实体映射为包含实时 SKU 上架汇总的响应。
    ///
    /// 名称、分类、品牌、维护人姓名与供给计数不在此填充。
    ///
    /// # 参数
    /// * `executor` - 原授权事务或读取阶段的执行器。
    /// * `product` - 已加载的商品实体
    ///
    /// # 返回
    /// 返回带继承上架状态和启用 SKU 计数的 `ProductView`；没有汇总时计数为零。
    ///
    /// # 错误
    /// 上架汇总读取失败时返回仓储错误。
    pub(super) async fn product_view(
        &self,
        product: Product,
        executor: &mut dyn Executor,
    ) -> Result<ProductView> {
        let product_id = ProductId::new(product.base.id.clone());
        let mut summaries = self.product_listing_views(std::slice::from_ref(&product_id), executor).await?;
        let summary =
            summaries.remove(product_id.as_ref()).unwrap_or(ProductListingView::new(product_id.to_string()));
        let source = self.db.product_sources().find_by_id(&product.base.id, executor).await?;
        Ok(ProductView {
            id: product.base.id,
            product_no: product.product_no,
            product_kind: product.product_kind,
            name: None,
            category_id: None,
            brand_id: None,
            status: product.stable.status,
            listing_status: summary.listing_status,
            listed_sku_count: summary.listed_sku_count,
            sku_count: summary.sku_count,
            supplied_sku_count: 0,
            priced_sku_count: 0,
            source_diff_count: source.as_ref().map_or(0, |s| s.pending_count),
            source_diff_fields: source.map_or_else(Vec::new, |s| s.pending_fields),
            current_revision_id: product.stable.current_revision_id,
            created_at: product.base.created_at,
            version: product.base.version,
            maintainer_user_id: product.maintainer_user_id,
            maintainer_user_name: None,
            business_org_unit_id: product.business_org_unit_id,
        })
    }
}

/// 从整组 SKU 中筛出状态实际发生变化的实体。
fn changed_skus(skus: Vec<Sku>, target: ListingStatus, actor_id: &str) -> Result<Vec<Sku>> {
    let mut changed = Vec::new();
    for mut sku in skus {
        if target.is_listed() && !sku.is_active() {
            continue;
        }
        if sku.set_listing_status(target, actor_id)? {
            changed.push(sku);
        }
    }
    Ok(changed)
}

/// 已完成整组切换后构造 SPU 继承状态。
fn product_listing_view(product_id: &str, sku_count: usize, target: ListingStatus) -> ProductListingView {
    let sku_count = u32::try_from(sku_count).unwrap_or(u32::MAX);
    let listed_sku_count = if target.is_listed() { sku_count } else { 0 };
    listing_view_from_counts(product_id, listed_sku_count, sku_count)
}

/// 把仓储上架计数映射为 SPU 继承状态视图。
///
/// # 参数
/// * `product_id` - 商品稳定主键
/// * `listed_sku_count` - 当前启用且已上架的 SKU 数
/// * `sku_count` - 当前启用 SKU 总数
///
/// # 返回
/// 返回 `ProductListingStatus::inherited` 计算后的响应视图。
///
/// # 错误
/// 无。
fn listing_view_from_counts(product_id: &str, listed_sku_count: u32, sku_count: u32) -> ProductListingView {
    ProductListingView {
        product_id: product_id.to_string(),
        listing_status: ProductListingStatus::inherited(listed_sku_count, sku_count),
        listed_sku_count,
        sku_count,
    }
}

/// 把批量上架汇总投影映射为按商品 ID 索引的响应视图。
///
/// # 参数
/// * `product_ids` - 请求的商品稳定 ID
/// * `summaries` - Repository 按首次出现顺序返回的上架计数
///
/// # 返回
/// 返回每个请求商品的继承上架视图；仓储已对缺项补零。
///
/// # 错误
/// 无。
fn product_listing_views_from_summaries(
    product_ids: &[ProductId],
    summaries: &[ProductListingSummary],
) -> HashMap<String, ProductListingView> {
    let mut views = HashMap::with_capacity(summaries.len());
    for summary in summaries {
        views.insert(
            summary.product_id.clone(),
            listing_view_from_counts(&summary.product_id, summary.listed_sku_count, summary.sku_count),
        );
    }
    for product_id in product_ids {
        views
            .entry(product_id.to_string())
            .or_insert_with(|| listing_view_from_counts(product_id.as_ref(), 0, 0));
    }
    views
}

#[cfg(test)]
mod tests {
    use erp_core::ids::ProductId;

    use super::*;
    use crate::entity::catalog::ProductListingStatus;

    fn summary(product_id: &str, listed_sku_count: u32, sku_count: u32) -> ProductListingSummary {
        ProductListingSummary::new(product_id, listed_sku_count, sku_count)
    }

    #[test]
    fn product_listing_view_new_starts_unlisted_with_zero_counts() {
        let view = ProductListingView::new("product-1");
        assert_eq!(view.product_id, "product-1");
        assert_eq!(view.listing_status, ProductListingStatus::Unlisted);
        assert_eq!(view.listed_sku_count, 0);
        assert_eq!(view.sku_count, 0);
    }

    #[test]
    fn product_listing_view_reports_batch_target() {
        let listed = product_listing_view("product-1", 3, ListingStatus::Listed);
        let unlisted = product_listing_view("product-1", 3, ListingStatus::Unlisted);

        assert_eq!(listed.listing_status, ProductListingStatus::Listed);
        assert_eq!(listed.listed_sku_count, 3);
        assert_eq!(unlisted.listing_status, ProductListingStatus::Unlisted);
        assert_eq!(unlisted.listed_sku_count, 0);
    }

    /// 零 SKU、全部停用、全部上架、部分上架、全部未上架及多商品批量共用 inherited 口径。
    #[test]
    fn listing_views_cover_zero_disabled_listed_partial_and_batch() {
        let product_ids = [
            ProductId::new("zero"),
            ProductId::new("all-listed"),
            ProductId::new("partial"),
            ProductId::new("all-unlisted"),
        ];
        let views = product_listing_views_from_summaries(
            &product_ids,
            &[summary("all-listed", 2, 2), summary("partial", 1, 2), summary("all-unlisted", 0, 2)],
        );

        assert_eq!(views["zero"].listing_status, ProductListingStatus::Unlisted);
        assert_eq!(views["zero"].sku_count, 0);
        assert_eq!(views["all-listed"].listing_status, ProductListingStatus::Listed);
        assert_eq!(views["partial"].listing_status, ProductListingStatus::PartiallyListed);
        assert_eq!(views["all-unlisted"].listing_status, ProductListingStatus::Unlisted);
        assert_eq!(views["all-unlisted"].sku_count, 2);
    }
}

#[cfg(test)]
mod authorization_tests {
    use erp_core::ids::{SkuId, UnitOfMeasureId};
    use persistence_core::testing::SnapshotExecutor;
    use serde_json::json;

    use super::super::testing::{actor, denied, service};
    use super::*;
    use crate::entity::catalog::sku::SkuData;
    use crate::entity::catalog::{EMPTY_SPEC_SIGNATURE, EnableStatus};

    #[tokio::test]
    async fn sku_version_conflicts_are_not_reported_before_product_authorization() {
        let service = service().await;
        let actor = actor();
        let sku = Sku::new(
            SkuId::new("sku"),
            SkuData {
                sku_no: "SKU-1".into(),
                product_id: ProductId::new("product"),
                base_unit_id: UnitOfMeasureId::new("unit"),
                specification_signature: EMPTY_SPEC_SIGNATURE.into(),
                status: EnableStatus::Active,
                listing_status: ListingStatus::Unlisted,
            },
            actor.id(),
        )
        .unwrap();
        for version in [sku.base.version, sku.base.version + 1] {
            denied(
                service
                    .authorize_sku_listing_change(&sku, version, &actor, &mut SnapshotExecutor::default())
                    .await,
            );
        }
    }

    #[tokio::test]
    async fn writes_authorize_before_existing_records_versions_or_related_facts() {
        let service = service().await;
        let actor = actor();
        let mut executor = SnapshotExecutor::default();
        for version in [1, 999] {
            denied(
                service
                    .product_listing_update_stage(
                        "missing",
                        serde_json::from_value(json!({"listing_status":"listed"})).unwrap(),
                        &actor,
                        &mut executor,
                    )
                    .await,
            );
            denied(
                service
                    .sku_listing_update_stage(
                        "missing",
                        serde_json::from_value(json!({"version":version,"listing_status":"listed"})).unwrap(),
                        &actor,
                        &mut executor,
                    )
                    .await,
            );
        }
    }
}
