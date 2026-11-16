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
mod procurement;
mod scope;
mod supplier_codes;

pub(crate) use procurement::{CategoryChainCache, matches_procurement_owner};
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
    /// 注入查询、范围与采购负责人解析；构造不读取事实。
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
    ///
    /// # 错误
    /// 无。
    pub fn new(
        db: Database,
        query: Arc<dyn CatalogSupplyQueryPort>,
        access: CatalogAccess,
        procurement: Arc<dyn ProductProcurementOwners>,
        authorization: Arc<AuthorizationService>,
    ) -> Self {
        Self { db, authorization, query, access, procurement }
    }

    /// 查询可售 SKU；资格日期由原商品领域逻辑确定。
    ///
    /// # 参数
    /// * `actor` - 本次已认证账号。
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

    /// 在调用方快照内查询可售 SKU，保留固定功能资格与完整分页语义。
    ///
    /// # 参数
    /// * `params` - 可售筛选，排序、分页和资格日期沿用商品领域校验。
    /// * `actor` - 当前已认证账号。
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
            return Err(Error::ValidationError("商品读取跨越授权快照".into()));
        }
        Ok(view)
    }
}

/// 商品池及配送提示只允许在活动快照中检查固定功能资格。
async fn require_sellable_read(
    authorization: &AuthorizationService,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<ExecutionBoundary> {
    if !executor.boundary().is_some_and(|boundary| boundary.is_snapshot()) {
        return Err(Error::ValidationError("商品读取必须使用快照事务".into()));
    }
    authorization
        .require(actor, &catalog::PRODUCT_SELLABLE_SKU_LIST, AccessTarget::Operation, executor)
        .await?;
    executor
        .boundary()
        .filter(|boundary| boundary.is_snapshot())
        .ok_or_else(|| Error::ValidationError("商品读取缺少授权快照".into()))
}

/// 按商品领域筛选查询并投影当前可售页，保留同序供应商身份供编号补齐。
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
