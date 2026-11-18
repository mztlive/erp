//! 来源同步只读取当前修订及有索引的最大修订序号。
use erp_core::ids::{ProductId, ProductRevisionId};
use mongodb::bson::doc;
use mongodb::options::FindOptions;
use persistence_core::{Executor, Result, aggregate};

use super::product::ProductDisableSnapshot;
use super::{CatalogReadScope, CatalogRepository, ProductRevisionMediaRepositoryExt};
use crate::entity::catalog::Product;
use crate::repository::CatalogExt;

impl CatalogRepository<'_> {
    /// 读取来源或商品修订之前，在原读取阶段按稳定 ID 应用当前商品详情凭证。
    /// 不存在或不在范围内均返回 `None`，不进一步加载来源；凭证或仓储失败原样返回。
    pub async fn authorized_product(
        &self,
        product_id: &str,
        scope: &CatalogReadScope,
        executor: &mut dyn Executor,
    ) -> crate::Result<Option<Product>> {
        scope.ensure_current(executor)?;
        let rows = aggregate::find(
            &self.db.products().collection(),
            doc! { "id": product_id },
            FindOptions::builder().limit(1).build(),
            scope.stages().to_vec(),
            executor,
        )
        .await?;
        scope.ensure_current(executor)?;
        Ok(rows.into_iter().next())
    }

    /// 来源同步准备展示资料更新时，沿原执行器读取商品、当前修订和媒体，并取历史最大修订号用于追加。
    /// 不加载完整修订历史，也不替代调用方的商品授权。
    pub async fn source_product_snapshot(
        &self,
        product_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<ProductDisableSnapshot>> {
        let Some(product) = self.db.products().find_by_id(product_id, executor).await? else {
            return Ok(None);
        };

        let current_revision = if let Some(id) = product.stable.current_revision_id.as_deref() {
            self.db.product_revisions().find_by_id(id, executor).await?
        } else {
            None
        };
        let latest_revision_no =
            self.latest_product_revision_no(&ProductId::new(product_id), executor).await?;

        let media = if let Some(revision) = &current_revision {
            self.db
                .product_revision_medias()
                .find_media_by_revision_ids(&[ProductRevisionId::new(revision.base.id.clone())], executor)
                .await?
        } else {
            Vec::new()
        };
        Ok(Some(ProductDisableSnapshot { product, current_revision, media, latest_revision_no }))
    }
}
