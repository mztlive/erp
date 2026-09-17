use persistence_core::NoTransaction;
use validator::Validate;

use super::CatalogService;
use crate::dto::{
    PageView, ProductListParams, ProductRevisionListParams, ProductRevisionView, ProductView, SkuListParams,
    SkuRevisionListParams, SkuRevisionView, SkuView, SortDir,
};
use crate::error::Result;
use crate::repository::CatalogExt;

/// 商品列表仓储筛选条件类型。
type ProductFilter = <mongodb::Database as CatalogExt>::ProductFilter;
/// 商品修订列表仓储筛选条件类型。
type ProductRevisionFilter = <mongodb::Database as CatalogExt>::ProductRevisionFilter;
/// SKU 列表仓储筛选条件类型。
type SkuFilter = <mongodb::Database as CatalogExt>::SkuFilter;
/// SKU 修订列表仓储筛选条件类型。
type SkuRevisionFilter = <mongodb::Database as CatalogExt>::SkuRevisionFilter;

impl CatalogService {
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
        let page = self.db.catalog().product_revision_page(&filter, &mut NoTransaction).await?;
        let items = page.items.into_iter().map(ProductRevisionView::from).collect();
        Ok(PageView { items, total: page.total, page: query.paging.page, page_size: query.paging.page_size })
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
        let page = self.db.catalog().sku_page(keyword.as_deref(), &filter, &mut NoTransaction).await?;
        let items = page.items.into_iter().map(SkuView::from).collect();
        Ok(PageView { items, total: page.total, page: query.paging.page, page_size: query.paging.page_size })
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
        let page = self.db.catalog().sku_revision_page(&filter, &mut NoTransaction).await?;
        let items = page.items.into_iter().map(SkuRevisionView::from).collect();
        Ok(PageView { items, total: page.total, page: query.paging.page, page_size: query.paging.page_size })
    }
}

/// 校验并规范化商品列表请求；保持原首错顺序。
pub fn prepare_product_list(params: &ProductListParams) -> Result<ProductFilter> {
    params.validate()?;
    let query = params.normalized()?;
    Ok(ProductFilter {
        ids: None,
        scope: None,
        maintainer_user_ids: query.owner_user_ids.map(|ids| ids.as_slice().to_vec()),
        business_org_unit_ids: query.org_unit_ids.map(|ids| ids.as_slice().to_vec()),
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
    })
}

/// 将已读取的商品行投影为原列表响应，不读取额外事实。
pub fn product_page_view(
    page: persistence_core::PageResult<crate::repository::ProductRow>,
    filter: &ProductFilter,
) -> Result<PageView<ProductView>> {
    let items = page.items.into_iter().map(ProductView::from).collect();
    Ok(PageView { items, total: page.total, page: filter.page, page_size: filter.page_size })
}
