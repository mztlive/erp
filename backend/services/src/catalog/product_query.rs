use std::collections::HashMap;

use database::{CatalogExt, NoTransaction};
use entities::catalog::ProductRevisionId;
use mongodb::bson::doc;
use validator::Validate;

use super::CatalogService;
use crate::catalog::dto::{
    PageView, ProductListParams, ProductRevisionListParams, ProductRevisionMediaView, ProductRevisionView,
    ProductView, SkuListParams, SkuRevisionListParams, SkuRevisionView, SkuView, SortDir,
};
use crate::errors::Result;

/// 商品列表筛选条件类型。
type ProductFilter = <mongodb::Database as CatalogExt>::ProductFilter;
/// 商品修订列表筛选条件类型。
type ProductRevisionFilter = <mongodb::Database as CatalogExt>::ProductRevisionFilter;
/// SKU 列表筛选条件类型。
type SkuFilter = <mongodb::Database as CatalogExt>::SkuFilter;
/// SKU 修订列表筛选条件类型。
type SkuRevisionFilter = <mongodb::Database as CatalogExt>::SkuRevisionFilter;

impl CatalogService {
    /// 分页查询商品列表。
    ///
    /// # 参数
    /// * `params` - 商品、当前修订与当前启用 SKU 的扁平筛选参数
    ///
    /// # 返回
    /// 返回契约形状的分页视图。
    ///
    /// # 错误
    /// * `ValidationError` - 分页参数非法或排序字段不在白名单
    pub async fn product_list(&self, params: &ProductListParams) -> Result<PageView<ProductView>> {
        params.validate()?;
        let query = params.normalized()?;
        let filter = ProductFilter {
            product_no: query.product_no,
            keyword: query.keyword,
            product_kind: query.product_kind,
            category_id: query.category_id,
            brand_id: query.brand_id,
            supplier_id: query.supplier_id,
            status: query.status,
            listing_status: query.listing_status,
            supply_coverage: query.supply_coverage,
            sales_price_min: query.sales_price_min,
            sales_price_max: query.sales_price_max,
            page: query.paging.page,
            page_size: query.paging.page_size,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: matches!(query.paging.sort_dir, SortDir::Asc),
        };
        let page = self
            .db
            .catalog()
            .search_products(&filter, &mut NoTransaction)
            .await?;
        let items = page
            .items
            .into_iter()
            .map(|row| ProductView {
                id: row.id,
                product_no: row.product_no,
                product_kind: row.product_kind,
                name: row.name,
                category_id: row.category_id,
                brand_id: row.brand_id,
                status: row.status,
                listing_status: row.listing_status,
                listed_sku_count: row.listed_sku_count,
                sku_count: row.sku_count,
                supplied_sku_count: row.supplied_sku_count,
                priced_sku_count: row.priced_sku_count,
                current_revision_id: row.current_revision_id,
                created_at: row.created_at,
                version: row.version,
            })
            .collect();
        Ok(PageView {
            items,
            total: page.total,
            page: filter.page,
            page_size: filter.page_size,
        })
    }

    /// 分页查询商品修订列表。
    ///
    /// # 参数
    /// * `params` - 查询参数（`product_id`/`status` 扁平筛选）
    ///
    /// # 返回
    /// 返回契约形状的分页视图。
    ///
    /// # 错误
    /// * `ValidationError` - 分页参数非法或排序字段不在白名单
    pub async fn product_revision_list(
        &self,
        params: &ProductRevisionListParams,
    ) -> Result<PageView<ProductRevisionView>> {
        params.validate()?;
        let query = params.normalized()?;
        let filter = ProductRevisionFilter {
            product_id: query.product_id,
            status: query.status,
            page: query.paging.page,
            page_size: query.paging.page_size,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: matches!(query.paging.sort_dir, SortDir::Asc),
        };
        let page = self
            .db
            .product_revisions()
            .search_product_revisions(&filter, &mut NoTransaction)
            .await?;
        let media_by_revision = self
            .media_by_revision_ids(
                &page
                    .items
                    .iter()
                    .map(|row| ProductRevisionId::new(row.id.clone()))
                    .collect::<Vec<_>>(),
            )
            .await?;
        // 投影行类型属于仓储私有子树（`repository/mod.rs` 冻结），按字段映射为响应视图。
        let items = page
            .items
            .into_iter()
            .map(|row| ProductRevisionView {
                id: row.id.clone(),
                product_id: row.product_id,
                revision_no: row.revision_no,
                name: row.name,
                description: row.description,
                specification: row.specification,
                category_id: row.category_id,
                brand_id: row.brand_id,
                status: row.status,
                effective_from: row.effective_from,
                effective_to: row.effective_to,
                media: media_by_revision.get(&row.id).cloned().unwrap_or_default(),
                created_at: row.created_at,
                version: row.version,
            })
            .collect();
        Ok(PageView {
            items,
            total: page.total,
            page: filter.page,
            page_size: filter.page_size,
        })
    }

    /// 分页查询 SKU 列表。
    ///
    /// # 参数
    /// * `params` - 查询参数（`sku_no`/`product_id`/`status`/`listing_status` 扁平筛选）
    ///
    /// # 返回
    /// 返回契约形状的分页视图。
    ///
    /// # 错误
    /// * `ValidationError` - 分页参数非法或排序字段不在白名单
    pub async fn sku_list(&self, params: &SkuListParams) -> Result<PageView<SkuView>> {
        params.validate()?;
        let query = params.normalized()?;
        let ids = match query.q.as_deref() {
            Some(keyword) => Some(self.resolve_sku_ids_by_keyword(keyword).await?),
            None => None,
        };
        let filter = SkuFilter {
            sku_no: query.sku_no,
            ids,
            product_id: query.product_id,
            status: query.status,
            listing_status: query.listing_status,
            page: query.paging.page,
            page_size: query.paging.page_size,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: matches!(query.paging.sort_dir, SortDir::Asc),
        };
        let page = self.db.skus().search_skus(&filter, &mut NoTransaction).await?;
        let names = self.current_sku_revision_names(&page.items).await?;
        let items = page
            .items
            .into_iter()
            .map(|row| SkuView {
                id: row.id,
                sku_no: row.sku_no,
                product_id: row.product_id,
                base_unit_id: row.base_unit_id,
                specification_signature: row.specification_signature,
                status: row.status,
                listing_status: row.listing_status,
                current_revision_id: row.current_revision_id.clone(),
                name: row
                    .current_revision_id
                    .as_ref()
                    .and_then(|id| names.get(id).cloned()),
                created_at: row.created_at,
                version: row.version,
            })
            .collect();
        Ok(PageView {
            items,
            total: page.total,
            page: filter.page,
            page_size: filter.page_size,
        })
    }

    /// 按关键字解析公司 SKU 主键（SKU 编号或当前修订名称，模糊且忽略大小写）。
    ///
    /// # 参数
    /// * `keyword` - 已去空白的搜索关键字
    ///
    /// # 返回
    /// 返回去重后的命中 SKU 主键集合（可为空）。
    ///
    /// # 错误
    /// 数据库查询失败时返回错误。
    async fn resolve_sku_ids_by_keyword(&self, keyword: &str) -> Result<Vec<String>> {
        let pattern = regex::escape(keyword);
        let by_no = self
            .db
            .skus()
            .find_many(
                doc! {
                    "deleted_at": 0_i64,
                    "sku_no": { "$regex": &pattern, "$options": "i" },
                },
                &mut NoTransaction,
            )
            .await?;
        let by_name = self
            .db
            .sku_revisions()
            .find_many(
                doc! {
                    "deleted_at": 0_i64,
                    "name": { "$regex": &pattern, "$options": "i" },
                },
                &mut NoTransaction,
            )
            .await?;
        let mut ids = by_no.into_iter().map(|sku| sku.base.id).collect::<Vec<_>>();
        for revision in by_name {
            ids.push(revision.sku_id.to_string());
        }
        ids.sort();
        ids.dedup();
        Ok(ids)
    }

    /// 读取当前页 SKU 的当前修订名称。
    ///
    /// # 参数
    /// * `rows` - 当前页 SKU 投影行
    ///
    /// # 返回
    /// 返回「修订 ID → 公司审核后的 SKU 名称」映射（可为空）。
    ///
    /// # 错误
    /// 数据库查询失败时返回错误。
    async fn current_sku_revision_names(&self, rows: &[database::SkuRow]) -> Result<HashMap<String, String>> {
        let revision_ids = rows
            .iter()
            .filter_map(|row| row.current_revision_id.clone())
            .collect::<Vec<_>>();
        if revision_ids.is_empty() {
            return Ok(HashMap::new());
        }
        let revisions = self
            .db
            .sku_revisions()
            .find_many(
                doc! {
                    "deleted_at": 0_i64,
                    "id": { "$in": revision_ids },
                },
                &mut NoTransaction,
            )
            .await?;
        Ok(revisions
            .into_iter()
            .map(|revision| (revision.base.id, revision.name))
            .collect())
    }

    /// 分页查询 SKU 修订列表。
    ///
    /// # 参数
    /// * `params` - 查询参数（`sku_id`/`name`/`barcode`/`status` 扁平筛选）
    ///
    /// # 返回
    /// 返回契约形状的分页视图。
    ///
    /// # 错误
    /// * `ValidationError` - 分页参数非法或排序字段不在白名单
    pub async fn sku_revision_list(
        &self,
        params: &SkuRevisionListParams,
    ) -> Result<PageView<SkuRevisionView>> {
        params.validate()?;
        let query = params.normalized()?;
        let filter = SkuRevisionFilter {
            sku_id: query.sku_id,
            name: query.name,
            barcode: query.barcode,
            status: query.status,
            page: query.paging.page,
            page_size: query.paging.page_size,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: matches!(query.paging.sort_dir, SortDir::Asc),
        };
        let page = self
            .db
            .sku_revisions()
            .search_sku_revisions(&filter, &mut NoTransaction)
            .await?;
        let items = page
            .items
            .into_iter()
            .map(|row| SkuRevisionView {
                id: row.id,
                sku_id: row.sku_id,
                revision_no: row.revision_no,
                name: row.name,
                description: row.description,
                specification: row.specification,
                barcode: row.barcode,
                source_main_image_asset_id: row.source_main_image_asset_id,
                weight_kg: row.weight_kg,
                volume_m3: row.volume_m3,
                status: row.status,
                sales_visible_price_gross: row.sales_visible_price_gross,
                market_price: row.market_price,
                effective_from: row.effective_from,
                effective_to: row.effective_to,
                created_at: row.created_at,
                version: row.version,
            })
            .collect();
        Ok(PageView {
            items,
            total: page.total,
            page: filter.page,
            page_size: filter.page_size,
        })
    }

    /// 按修订 ID 批量读取媒体行并映射为响应视图（按修订分组）。
    ///
    /// # 参数
    /// * `revision_ids` - 商品修订 ID 集合
    ///
    /// # 返回
    /// 返回 `修订 ID → 媒体视图列表` 的分组映射。
    ///
    /// # 错误
    /// MongoDB 查询失败时返回错误。
    async fn media_by_revision_ids(
        &self,
        revision_ids: &[ProductRevisionId],
    ) -> Result<HashMap<String, Vec<ProductRevisionMediaView>>> {
        if revision_ids.is_empty() {
            return Ok(HashMap::new());
        }
        let rows = self
            .db
            .product_revision_medias()
            .find_media_by_revision_ids(revision_ids, &mut NoTransaction)
            .await?;
        let mut by_revision: HashMap<String, Vec<ProductRevisionMediaView>> = HashMap::new();
        for row in rows {
            by_revision
                .entry(row.product_revision_id.to_string())
                .or_default()
                .push(ProductRevisionMediaView {
                    id: row.base.id,
                    file_asset_id: row.file_asset_id.to_string(),
                    media_role: row.media_role,
                    sort_order: row.sort_order,
                    alt_text: row.alt_text,
                });
        }
        for views in by_revision.values_mut() {
            views.sort_by_key(|view| view.sort_order);
        }
        Ok(by_revision)
    }
}
