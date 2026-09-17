//! 商品详情子资源只读聚合（`product:detail` 专属，不经过列表聚合管道）。
//!
//! 修订/SKU/SKU 修订均强制按路径商品过滤；子资源先校验归属，跨商品访问一律 404。
//! 单商品详情走读模型列表聚合（`CatalogCenterReadService::product_detail`），
//! 与列表共用供给/价格计数口径，本域不再组装 `ProductView`。

use application_core::AuditActor;
use erp_core::ids::{ProductId, SkuId};
use persistence_core::NoTransaction;

use super::CatalogService;
use crate::dto::{
    PageView, ProductRevisionListParams, ProductRevisionView, SkuListParams, SkuRevisionListParams,
    SkuRevisionView, SkuView,
};
use crate::error::{Error, Result};
use crate::repository::CatalogExt;

impl CatalogService {
    /// 读取指定商品的全部修订（强制路径商品，倒序）。
    ///
    /// # 参数
    /// * `product_id` - 路径商品稳定 ID
    /// * `actor` - 已认证操作人
    ///
    /// # 返回
    /// 返回带媒体装配的修订分页视图。
    ///
    /// # 错误
    /// 商品不存在或不在范围内时返回 `NotFound`。
    pub async fn product_detail_revisions(
        &self,
        product_id: &str,
        actor: &AuditActor,
    ) -> Result<PageView<ProductRevisionView>> {
        self.require_visible_product(actor, product_id).await?;
        let params = ProductRevisionListParams {
            product_id: Some(ProductId::new(product_id.to_string())),
            status: None,
            page: Some(1),
            page_size: Some(100),
            sort_by: Some("revision_no".to_string()),
            sort_dir: Some("desc".to_string()),
        };
        self.product_revision_list(&params).await
    }

    /// 读取指定商品的全部 SKU。
    ///
    /// # 参数
    /// * `product_id` - 路径商品稳定 ID
    /// * `actor` - 已认证操作人
    ///
    /// # 返回
    /// 返回带当前修订名称的 SKU 分页视图。
    ///
    /// # 错误
    /// 商品不存在或不在范围内时返回 `NotFound`。
    pub async fn product_detail_skus(
        &self,
        product_id: &str,
        actor: &AuditActor,
    ) -> Result<PageView<SkuView>> {
        self.require_visible_product(actor, product_id).await?;
        let params = SkuListParams {
            q: None,
            sku_no: None,
            product_id: Some(ProductId::new(product_id.to_string())),
            status: None,
            listing_status: None,
            page: Some(1),
            page_size: Some(100),
            sort_by: None,
            sort_dir: None,
        };
        self.sku_list(&params).await
    }

    /// 读取指定商品下的 SKU 修订。
    ///
    /// # 参数
    /// * `product_id` - 路径商品稳定 ID
    /// * `sku_id` - 可选 SKU 过滤；提供时必须属于路径商品
    /// * `actor` - 已认证操作人
    ///
    /// # 返回
    /// 返回按修订号倒序的 SKU 修订分页视图。
    ///
    /// # 错误
    /// 商品或 SKU 不存在或不在范围内、SKU 不属于路径商品时返回 `NotFound`。
    pub async fn product_detail_sku_revisions(
        &self,
        product_id: &str,
        sku_id: Option<&str>,
        actor: &AuditActor,
    ) -> Result<PageView<SkuRevisionView>> {
        self.require_visible_product(actor, product_id).await?;
        if let Some(sku_id) = sku_id {
            return self.product_sku_revisions(product_id, sku_id).await;
        }
        self.merged_product_sku_revisions(product_id).await
    }

    async fn require_visible_product(&self, actor: &AuditActor, product_id: &str) -> Result<()> {
        self.access().require_product(actor, "detail", product_id, &mut NoTransaction).await?;
        Ok(())
    }

    async fn product_sku_revisions(
        &self,
        product_id: &str,
        sku_id: &str,
    ) -> Result<PageView<SkuRevisionView>> {
        // IDOR：SKU 归属不符时与商品不存在同码，避免枚举他品 SKU。
        let sku = self
            .db
            .skus()
            .find_by_id(sku_id, &mut NoTransaction)
            .await?
            .filter(|sku| sku.product_id.as_ref() == product_id)
            .ok_or_else(|| Error::NotFound("商品不存在".to_string()))?;
        self.sku_revision_list(&sku_revision_params(sku.base.id)).await
    }

    async fn merged_product_sku_revisions(&self, product_id: &str) -> Result<PageView<SkuRevisionView>> {
        let skus = self
            .db
            .catalog()
            .skus_for_product(&ProductId::new(product_id.to_string()), &mut NoTransaction)
            .await?;
        if skus.is_empty() {
            return Ok(PageView { items: Vec::new(), total: 0, page: 1, page_size: 100 });
        }
        let sku_ids = skus.iter().map(|sku| SkuId::new(sku.base.id.clone())).collect::<Vec<_>>();
        let mut revisions = self.db.sku_revisions().find_by_sku_ids(&sku_ids, &mut NoTransaction).await?;
        let total = revisions.len() as i64;
        revisions.sort_by(|left, right| {
            right
                .revision
                .revision_no
                .cmp(&left.revision.revision_no)
                .then_with(|| right.base.created_at.cmp(&left.base.created_at))
        });
        revisions.truncate(MERGED_SKU_REVISION_LIMIT);
        let items = revisions.into_iter().map(SkuRevisionView::from).collect();
        Ok(PageView { items, total, page: 1, page_size: 100 })
    }
}

/// 合并 SKU 修订明细返回上限（erp-catalog-009）。
///
/// `merged_product_sku_revisions` 一次 `$in` 拉回该商品全部 SKU 修订，
/// `total` 累计全量，`items` 仅返回排序前 100 条明细。
const MERGED_SKU_REVISION_LIMIT: usize = 100;

fn sku_revision_params(sku_id: String) -> SkuRevisionListParams {
    SkuRevisionListParams {
        sku_id: Some(SkuId::new(sku_id)),
        name: None,
        barcode: None,
        status: None,
        page: Some(1),
        page_size: Some(100),
        sort_by: Some("revision_no".to_string()),
        sort_dir: Some("desc".to_string()),
    }
}
