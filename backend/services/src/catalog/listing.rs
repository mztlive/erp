//! SKU 上下架与 SPU 继承状态编排。
//!
//! `product` 不重复持久化上架字段；SPU 状态始终从当前启用 SKU 汇总。
//! 整组切换在一个事务内更新全部目标 SKU 并写一条审计日志。

use std::collections::HashMap;

use database::{AccessControlExt, CatalogExt, NoTransaction, Transactional};
use entities::catalog::{ListingStatus, Product, ProductListingStatus, Sku};
use entities::ids::ProductId;
use validator::Validate;

use super::{ensure_version, CatalogService};
use crate::audit::AuditActor;
use crate::catalog::dto::{
    ProductListingView, ProductView, SkuView, UpdateProductListingRequest, UpdateSkuListingRequest,
};
use crate::errors::{Error, Result};

/// 整组上/下架在事务外完成校验后形成的待写集合。
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
        let change = self
            .prepare_product_listing_change(product_id, req.listing_status, actor.id())
            .await?;
        if change.changed.is_empty() {
            return Ok(change.view);
        }
        self.write_product_listing_change(change, req.listing_status, actor)
            .await
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
        req.validate()?;
        let (sku, changed) = self
            .prepare_sku_listing_change(sku_id, req.version, req.listing_status, actor.id())
            .await?;
        if !changed {
            return Ok(sku.into());
        }
        let sku = self
            .write_sku_listing_change(sku, req.listing_status, actor)
            .await?;
        Ok(sku.into())
    }

    /// 校验整组操作并构造需要持久化的 SKU 集合。
    async fn prepare_product_listing_change(
        &self,
        product_id: &str,
        target: ListingStatus,
        actor_id: &str,
    ) -> Result<ProductListingChange> {
        let product = self.load_product(product_id).await?;
        if target.is_listed() && !product.is_active() {
            return Err(Error::BusinessLogicError("停用的商品不能上架".to_string()));
        }
        let product_id = ProductId::new(product.base.id);
        let skus = self
            .db
            .skus()
            .find_by_product_ids(std::slice::from_ref(&product_id), &mut NoTransaction)
            .await?;
        let active_count = skus.iter().filter(|sku| sku.is_active()).count();
        if target.is_listed() && active_count == 0 {
            return Err(Error::BusinessLogicError("商品下没有可上架的 SKU".to_string()));
        }
        let changed = changed_skus(skus, target, actor_id)?;
        let view = product_listing_view(product_id.as_ref(), active_count, target);
        Ok(ProductListingChange {
            product_id,
            changed,
            view,
        })
    }

    /// 在一个事务内写入整组 SKU 状态与审计日志。
    async fn write_product_listing_change(
        &self,
        change: ProductListingChange,
        target: ListingStatus,
        actor: &AuditActor,
    ) -> Result<ProductListingView> {
        let ProductListingChange {
            product_id,
            mut changed,
            view,
        } = change;
        let audit = actor.clone().resource_log_with_message(
            "product.listing.update",
            "product",
            product_id.to_string(),
            Some(format!("整组{}", target.label())),
        )?;
        let db = self.db.clone();
        let client = db.client().clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    for sku in &mut changed {
                        db.skus().update(sku, session).await?;
                    }
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(), crate::errors::Error>(())
                })
            })
            .await?;
        Ok(view)
    }

    /// 校验单 SKU 操作并应用领域状态迁移。
    async fn prepare_sku_listing_change(
        &self,
        sku_id: &str,
        version: u64,
        target: ListingStatus,
        actor_id: &str,
    ) -> Result<(Sku, bool)> {
        let Some(mut sku) = self.db.skus().find_by_id(sku_id, &mut NoTransaction).await? else {
            return Err(Error::NotFound("SKU 不存在".to_string()));
        };
        ensure_version(sku.base.version, version)?;
        let product = self.load_product(sku.product_id.as_ref()).await?;
        if target.is_listed() && !product.is_active() {
            return Err(Error::BusinessLogicError("停用商品下的 SKU 不能上架".to_string()));
        }
        let changed = sku.set_listing_status(target, actor_id)?;
        Ok((sku, changed))
    }

    /// 在一个事务内写入单 SKU 状态与审计日志。
    async fn write_sku_listing_change(
        &self,
        mut sku: Sku,
        target: ListingStatus,
        actor: &AuditActor,
    ) -> Result<Sku> {
        let audit = actor.clone().resource_log_with_message(
            "sku.listing.update",
            "sku",
            sku.base.id.clone(),
            Some(target.label().to_string()),
        )?;
        let db = self.db.clone();
        let client = db.client().clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.skus().update(&mut sku, session).await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<Sku, crate::errors::Error>(sku)
                })
            })
            .await
    }

    /// 为一组 SPU 批量计算继承上架状态，避免列表 N+1。
    pub(super) async fn product_listing_views(
        &self,
        product_ids: &[ProductId],
    ) -> Result<HashMap<String, ProductListingView>> {
        let skus = self
            .db
            .skus()
            .find_by_product_ids(product_ids, &mut NoTransaction)
            .await?;
        let mut counts = HashMap::<String, (u32, u32)>::new();
        for sku in skus.into_iter().filter(Sku::is_active) {
            let entry = counts.entry(sku.product_id.to_string()).or_default();
            entry.1 = entry.1.saturating_add(1);
            if sku.listing_status.is_listed() {
                entry.0 = entry.0.saturating_add(1);
            }
        }
        Ok(product_ids
            .iter()
            .map(|product_id| {
                let (listed_sku_count, sku_count) =
                    counts.get(product_id.as_ref()).copied().unwrap_or_default();
                (
                    product_id.to_string(),
                    ProductListingView {
                        product_id: product_id.to_string(),
                        listing_status: ProductListingStatus::inherited(listed_sku_count, sku_count),
                        listed_sku_count,
                        sku_count,
                    },
                )
            })
            .collect())
    }

    /// 将商品实体映射为包含实时 SKU 上架汇总的响应。
    pub(super) async fn product_view(&self, product: Product) -> Result<ProductView> {
        let product_id = ProductId::new(product.base.id.clone());
        let mut summaries = self
            .product_listing_views(std::slice::from_ref(&product_id))
            .await?;
        let summary = summaries
            .remove(product_id.as_ref())
            .unwrap_or(ProductListingView {
                product_id: product_id.to_string(),
                listing_status: ProductListingStatus::Unlisted,
                listed_sku_count: 0,
                sku_count: 0,
            });
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
            current_revision_id: product.stable.current_revision_id,
            created_at: product.base.created_at,
            version: product.base.version,
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
    ProductListingView {
        product_id: product_id.to_string(),
        listing_status: ProductListingStatus::inherited(listed_sku_count, sku_count),
        listed_sku_count,
        sku_count,
    }
}

#[cfg(test)]
mod tests {
    use entities::catalog::ProductListingStatus;

    use super::*;

    #[test]
    fn product_listing_view_reports_batch_target() {
        let listed = product_listing_view("product-1", 3, ListingStatus::Listed);
        let unlisted = product_listing_view("product-1", 3, ListingStatus::Unlisted);

        assert_eq!(listed.listing_status, ProductListingStatus::Listed);
        assert_eq!(listed.listed_sku_count, 3);
        assert_eq!(unlisted.listing_status, ProductListingStatus::Unlisted);
        assert_eq!(unlisted.listed_sku_count, 0);
    }
}
