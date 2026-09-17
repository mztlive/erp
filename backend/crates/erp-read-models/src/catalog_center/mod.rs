//! 商品中心聚合读取；查询规则和响应形状由商品领域提供。
use std::sync::Arc;

use erp_catalog::CatalogDataScopePort;
use erp_catalog::ports::supply::CatalogSupplyQueryPort;
use mongodb::Database;

use crate::Result;

mod procurement;
mod scope;

pub use procurement::{MapProductProcurementOwners, MongoProductProcurementOwners, ProductProcurementOwners};

/// 商品列表与可售商品列表的组合入口。
pub struct CatalogCenterReadService {
    db: Option<Database>,
    query: Arc<dyn CatalogSupplyQueryPort>,
    data_scope: Arc<dyn CatalogDataScopePort>,
    procurement: Arc<dyn ProductProcurementOwners>,
}

impl CatalogCenterReadService {
    /// 注入查询、范围与采购负责人解析；构造不读取事实。
    ///
    /// # 参数
    /// * `db` - 商品与身份集合
    /// * `query` - 商品聚合查询
    /// * `data_scope` - 商品范围 Port
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
        data_scope: Arc<dyn CatalogDataScopePort>,
        procurement: Arc<dyn ProductProcurementOwners>,
    ) -> Self {
        Self { db: Some(db), query, data_scope, procurement }
    }

    /// 仅装配查询端口，供不解析范围的可售列表测试使用。
    #[cfg(test)]
    pub fn for_query(query: Arc<dyn CatalogSupplyQueryPort>) -> Self {
        Self {
            db: None,
            query,
            data_scope: erp_catalog::FailClosedCatalogDataScopePort::shared(),
            procurement: Arc::new(MapProductProcurementOwners::default()),
        }
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
        params: &erp_catalog::SellableSkuListParams,
    ) -> Result<erp_catalog::PageView<erp_catalog::SellableSkuView>> {
        let filter = erp_catalog::service::catalog::prepare_sellable_sku_list(params)?;
        let page = self
            .query
            .search_sellable_skus(&filter, &mut persistence_core::NoTransaction)
            .await
            .map_err(erp_catalog::Error::from)?;
        Ok(erp_catalog::service::catalog::sellable_sku_page_view(page, &filter)?)
    }
}

#[cfg(test)]
mod tests;
