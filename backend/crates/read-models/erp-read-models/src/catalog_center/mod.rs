//! 商品中心聚合读取；查询规则和响应形状由商品领域提供。
use std::sync::Arc;

use erp_catalog::ports::supply::CatalogSupplyQueryPort;
use erp_catalog::service::catalog::{prepare_sellable_sku_list, sellable_sku_page_view};
use erp_catalog::{CatalogAccess, Error as CatalogError, PageView, SellableSkuListParams, SellableSkuView};
use mongodb::Database;
use persistence_core::NoTransaction;

use crate::Result;

pub mod delivery_coverage;
mod procurement;
mod scope;
mod supplier_codes;

pub(crate) use procurement::{CategoryChainCache, matches_procurement_owner};
pub use procurement::{MapProductProcurementOwners, MongoProductProcurementOwners, ProductProcurementOwners};

/// 商品列表与可售商品列表的组合入口。
pub struct CatalogCenterReadService {
    db: Database,
    query: Arc<dyn CatalogSupplyQueryPort>,
    access: CatalogAccess,
    procurement: Arc<dyn ProductProcurementOwners>,
}

impl CatalogCenterReadService {
    /// 注入查询、范围与采购负责人解析；构造不读取事实。
    ///
    /// # 参数
    /// * `db` - 商品与身份集合
    /// * `query` - 商品聚合查询
    /// * `access` - 已装配的商品授权服务
    /// * `procurement` - 采购负责人规则解析
    ///
    /// # 返回
    /// 返回未执行 I/O 的读取服务。
    ///
    /// # 错误
    /// 无。
    pub fn new(
        db: Database,
        query: Arc<dyn CatalogSupplyQueryPort>,
        access: CatalogAccess,
        procurement: Arc<dyn ProductProcurementOwners>,
    ) -> Self {
        Self { db, query, access, procurement }
    }

    /// 查询可售 SKU；资格日期由原商品领域逻辑确定。
    ///
    /// # 参数
    /// * `params` - 可售筛选
    ///
    /// # 返回
    /// 返回可售 SKU 分页。
    ///
    /// # 错误
    /// 筛选非法或仓储失败时拒绝。
    pub async fn sellable_sku_list(
        &self,
        params: &SellableSkuListParams,
    ) -> Result<PageView<SellableSkuView>> {
        let (mut view, supplier_ids) = sellable_sku_page(self.query.as_ref(), params).await?;
        let numbers = self.supplier_numbers(&supplier_ids).await?;
        supplier_codes::apply_supplier_codes(&mut view.items, &supplier_ids, &numbers);
        Ok(view)
    }
}

/// 按商品领域筛选查询并投影当前可售页，保留同序供应商身份供编号补齐。
async fn sellable_sku_page(
    query: &dyn CatalogSupplyQueryPort,
    params: &SellableSkuListParams,
) -> Result<(PageView<SellableSkuView>, Vec<Vec<String>>)> {
    let filter = prepare_sellable_sku_list(params)?;
    let page = query.search_sellable_skus(&filter, &mut NoTransaction).await.map_err(CatalogError::from)?;
    let supplier_ids = page.items.iter().map(|row| row.supplier_ids.clone()).collect();
    Ok((sellable_sku_page_view(page, &filter)?, supplier_ids))
}

#[cfg(test)]
mod tests;
