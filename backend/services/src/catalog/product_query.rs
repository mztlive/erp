use database::{CatalogExt, NoTransaction};
use validator::Validate;

use super::CatalogService;
use crate::catalog::dto::{
    PageView, ProductListParams, ProductRevisionListParams, ProductRevisionMediaView, ProductRevisionView,
    ProductView, SkuListParams, SkuRevisionListParams, SkuRevisionView, SkuView, SortDir,
};
use crate::errors::Result;

/// 商品列表仓储筛选条件类型。
type ProductFilter = <mongodb::Database as CatalogExt>::ProductFilter;
/// 商品修订列表仓储筛选条件类型。
type ProductRevisionFilter = <mongodb::Database as CatalogExt>::ProductRevisionFilter;
/// SKU 列表仓储筛选条件类型。
type SkuFilter = <mongodb::Database as CatalogExt>::SkuFilter;
/// SKU 修订列表仓储筛选条件类型。
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
    /// 分页、价格区间或排序参数非法，以及仓储查询失败时返回错误。
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
            .product_page(&filter, &mut NoTransaction)
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
            page: query.paging.page,
            page_size: query.paging.page_size,
        })
    }

    /// 分页查询商品修订列表。
    ///
    /// # 参数
    /// * `params` - 商品、状态、分页与排序筛选参数
    ///
    /// # 返回
    /// 返回已批量装配 SPU 级媒体的商品修订分页视图。
    ///
    /// # 错误
    /// 分页或排序参数非法，以及仓储查询失败时返回错误。
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
            .catalog()
            .product_revision_page(&filter, &mut NoTransaction)
            .await?;
        let items = page
            .items
            .into_iter()
            .map(|row| ProductRevisionView {
                id: row.id,
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
                media: row
                    .media
                    .into_iter()
                    .map(|media| ProductRevisionMediaView {
                        id: media.base.id,
                        file_asset_id: media.file_asset_id.to_string(),
                        media_role: media.media_role,
                        sort_order: media.sort_order,
                        alt_text: media.alt_text,
                    })
                    .collect(),
                created_at: row.created_at,
                version: row.version,
            })
            .collect();
        Ok(PageView {
            items,
            total: page.total,
            page: query.paging.page,
            page_size: query.paging.page_size,
        })
    }

    /// 分页查询 SKU 列表。
    ///
    /// # 参数
    /// * `params` - SKU 编号、关键字、商品、状态、分页与排序筛选参数
    ///
    /// # 返回
    /// 返回已批量装配当前修订名称的 SKU 分页视图。
    ///
    /// # 错误
    /// 分页或排序参数非法，以及仓储查询失败时返回错误。
    pub async fn sku_list(&self, params: &SkuListParams) -> Result<PageView<SkuView>> {
        params.validate()?;
        let query = params.normalized()?;
        let keyword = query.q;
        let filter = SkuFilter {
            sku_no: query.sku_no,
            ids: None,
            product_id: query.product_id,
            status: query.status,
            listing_status: query.listing_status,
            page: query.paging.page,
            page_size: query.paging.page_size,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: matches!(query.paging.sort_dir, SortDir::Asc),
        };
        let page = self
            .db
            .catalog()
            .sku_page(keyword.as_deref(), &filter, &mut NoTransaction)
            .await?;
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
                current_revision_id: row.current_revision_id,
                name: row.name,
                created_at: row.created_at,
                version: row.version,
            })
            .collect();
        Ok(PageView {
            items,
            total: page.total,
            page: query.paging.page,
            page_size: query.paging.page_size,
        })
    }

    /// 分页查询 SKU 修订列表。
    ///
    /// # 参数
    /// * `params` - SKU、名称、条码、状态、分页与排序筛选参数
    ///
    /// # 返回
    /// 返回 SKU 修订分页视图。
    ///
    /// # 错误
    /// 分页或排序参数非法，以及仓储查询失败时返回错误。
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
            .catalog()
            .sku_revision_page(&filter, &mut NoTransaction)
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
            page: query.paging.page,
            page_size: query.paging.page_size,
        })
    }
}
