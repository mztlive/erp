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
    /// 按真实商品身份执行完整授权条件，不读取无权商品的来源或修订。
    /// # 参数
    /// * `product_id` - 商品稳定身份。
    /// * `scope` - 当前读取阶段的商品详情查询凭证。
    /// * `executor` - 与授权相同的读取执行器。
    /// # 返回
    /// 授权范围内的商品；缺失或无权时为 None。
    /// # 错误
    /// 凭证失效、数据库查询或解码失败时返回错误。
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

    /// 读取来源同步需要的当前快照，不加载商品全部历史修订。
    /// # 参数
    /// 商品身份和调用方事务执行器。
    /// # 返回
    /// 商品、当前修订、当前媒体及历史最大修订序号。
    /// # 错误
    /// 数据库读取失败时拒绝。
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
