//! 商品中心聚合读取；查询规则和响应形状由商品领域提供。
use std::sync::Arc;

use application_core::AuditActor;
use erp_catalog::ports::supply::CatalogSupplyQueryPort;
use erp_catalog::service::catalog::{prepare_sellable_sku_list, sellable_sku_page_view};
use erp_catalog::{CatalogAccess, Error as CatalogError, PageView, SellableSkuListParams, SellableSkuView};
use erp_identity::{AccessTarget, AuthorizationService};
use erp_permissions::catalog;
use mongodb::Database;
use persistence_core::{ExecutionBoundary, Executor, Transactional};

use crate::{Error, Result};

pub mod delivery_coverage;
pub(crate) mod procurement;
mod scope;
mod supplier_codes;

pub use procurement::{MapProductProcurementOwners, MongoProductProcurementOwners, ProductProcurementOwners};

/// 商品列表与可售商品列表的组合入口。
#[derive(Clone)]
pub struct CatalogCenterReadService {
    db: Database,
    authorization: Arc<AuthorizationService>,
    query: Arc<dyn CatalogSupplyQueryPort>,
    access: CatalogAccess,
    procurement: Arc<dyn ProductProcurementOwners>,
}

impl CatalogCenterReadService {
    /// 装配商品中心时，注入查询、范围与采购负责人解析；构造不读取事实。
    ///
    /// # 参数
    /// * `authorization` - 组合根构造并共享的统一授权服务。
    /// * `db` - 商品与身份集合
    /// * `query` - 商品聚合查询
    /// * `access` - 已装配的商品授权服务
    /// * `procurement` - 采购负责人规则解析
    ///
    /// # 返回
    /// 返回未执行 I/O 的读取服务。
    pub fn new(
        db: Database,
        query: Arc<dyn CatalogSupplyQueryPort>,
        access: CatalogAccess,
        procurement: Arc<dyn ProductProcurementOwners>,
        authorization: Arc<AuthorizationService>,
    ) -> Self {
        Self { db, authorization, query, access, procurement }
    }

    /// 打开独立的可售 SKU 列表时，开启读取事务执行固定资格检查和分页查询，资格日期由商品领域逻辑确定。
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
        actor: &AuditActor,
    ) -> Result<PageView<SellableSkuView>> {
        let service = self.clone();
        let actor = actor.clone();
        let params = params.clone();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move { service.sellable_sku_list_with(&params, &actor, executor).await })
            })
            .await
    }

    /// 调用方已有快照时，沿原执行器检查商品读取资格、查询可售页并补齐供应商编号；只在原执行边界仍有效时交付。
    ///
    /// # 参数
    /// * `params` - 可售筛选，排序、分页和资格日期沿用商品领域校验。
    /// * `executor` - 调用方活动快照执行器，资格、分页、计数与展示补齐共用它。
    ///
    /// # 返回
    /// 返回完成供应商编号补齐的原分页视图，成员、行序及总数保持原值。
    ///
    /// # 错误
    /// 非快照执行器、缺少商品读取资格、筛选非法或关联读取失败时拒绝。
    pub async fn sellable_sku_list_with(
        &self,
        params: &SellableSkuListParams,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<PageView<SellableSkuView>> {
        let boundary = require_sellable_read(&self.authorization, actor, executor).await?;

        let (mut view, supplier_ids) = sellable_sku_page(self.query.as_ref(), params, executor).await?;
        let numbers = self.supplier_numbers(&supplier_ids, executor).await?;
        supplier_codes::apply_supplier_codes(&mut view.items, &supplier_ids, &numbers);

        if !boundary.is_current(executor) {
            return Err(Error::ValidationError("本次商品查询已失效，请刷新商品列表后重新查询".into()));
        }
        Ok(view)
    }
}

// 查询商品池或配送提示前，先确认执行器处于活动快照，再核对两个入口共用的固定功能资格。
async fn require_sellable_read(
    authorization: &AuthorizationService,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<ExecutionBoundary> {
    if !executor.boundary().is_some_and(|boundary| boundary.is_snapshot()) {
        return Err(Error::ValidationError("暂时无法查询可售商品，请从商品列表重新查询".into()));
    }
    authorization
        .require(actor, &catalog::PRODUCT_SELLABLE_SKU_LIST, AccessTarget::Operation, executor)
        .await?;
    executor.boundary().filter(|boundary| boundary.is_snapshot()).ok_or_else(|| {
        Error::ValidationError("暂时无法确认当前账号可查看的商品，请刷新商品列表后重新查询".into())
    })
}

// 可售商品读取资格确认后，按领域条件查询当前页，并另外返回同序的供应商身份供编号补齐。
async fn sellable_sku_page(
    query: &dyn CatalogSupplyQueryPort,
    params: &SellableSkuListParams,
    executor: &mut dyn Executor,
) -> Result<(PageView<SellableSkuView>, Vec<Vec<String>>)> {
    let filter = prepare_sellable_sku_list(params)?;
    let page = query.search_sellable_skus(&filter, executor).await.map_err(CatalogError::from)?;
    let supplier_ids = page.items.iter().map(|row| row.supplier_ids.clone()).collect();
    Ok((sellable_sku_page_view(page, &filter)?, supplier_ids))
}

#[cfg(test)]
mod tests;
