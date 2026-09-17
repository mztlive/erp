//! 商品列表与详情的一致授权快照；范围与业务版本跨页携带。

use std::hash::{Hash, Hasher};
use std::sync::Arc;

use application_core::{AuditActor, FilterOption, FilteredPage};
use erp_catalog::ports::supply::CatalogSupplyQueryPort;
use erp_catalog::service::catalog::{prepare_product_list, product_page_view};
use erp_catalog::{
    CatalogAccess, CatalogDataScopePort, CatalogExt, CatalogReadScope, ProductFilter, ProductListParams,
    ProductListView, ProductView,
};
use erp_identity::AccessControlExt;
use persistence_core::Transactional;

use super::CatalogCenterReadService;
use super::procurement::ProductProcurementOwners;
use crate::{Error, Result};

impl CatalogCenterReadService {
    /// 分页查询授权范围内的商品列表。
    ///
    /// # 参数
    /// * `params` - 商品筛选与范围参数
    /// * `actor` - 已认证操作人
    ///
    /// # 返回
    /// 返回分页、维护人候选、采购负责人候选与范围元信息。
    ///
    /// # 错误
    /// 缺范围版本的后续页、范围变化、筛选非法或仓储失败时拒绝。
    pub async fn product_list(
        &self,
        params: &ProductListParams,
        actor: &AuditActor,
    ) -> Result<ProductListView> {
        if params.page.unwrap_or(1) > 1 && params.scope_version.as_deref().is_none_or(str::is_empty) {
            return Err(data_scope_changed("请从第一页刷新后继续查询"));
        }
        let snapshot = self.list_snapshot(params, actor).await?;
        if params.scope_version.as_deref().is_some_and(|value| value != snapshot.scope_version) {
            return Err(data_scope_changed("数据范围已变化，请从第一页刷新"));
        }
        Ok(ProductListView {
            data: FilteredPage {
                owner_options: snapshot.owner_options,
                ownership_basis: "product_maintainer",
                page: snapshot.page,
            },
            procurement_owner_options: snapshot.procurement_owner_options,
            scope_version: snapshot.scope_version,
            policy_version: snapshot.policy_version,
            organization_version: snapshot.organization_version,
            as_of: snapshot.as_of,
            empty_reason: snapshot.no_scope.then_some("no_scope"),
            scope_summary: "商品维护人及其业务组织范围；采购负责人筛选为额外收窄",
        })
    }

    /// 按稳定 ID 查询单个商品，独立按 detail 动作解析。
    ///
    /// # 参数
    /// * `id` - 商品稳定 ID
    /// * `actor` - 已认证操作人
    ///
    /// # 返回
    /// 返回与列表同构的单个商品视图。
    ///
    /// # 错误
    /// 不存在或不在范围内返回 NotFound。
    pub async fn product_detail(&self, id: &str, actor: &AuditActor) -> Result<ProductView> {
        let db = self.db.clone().ok_or_else(|| Error::Internal("商品中心未装配数据库".into()))?;
        let access = CatalogAccess::new(db, self.data_scope.clone());
        let product =
            access.require_product(actor, "detail", id, &mut persistence_core::NoTransaction).await?;
        let mut filter = prepare_product_list(&ProductListParams {
            page: Some(1),
            page_size: Some(1),
            ..ProductListParams::default()
        })?;
        filter.ids = Some(vec![product.base.id.clone()]);
        let page = self.query.product_page(&filter, &mut persistence_core::NoTransaction).await?;
        let view = product_page_view(page, &filter)?;
        view.items.into_iter().next().ok_or_else(|| Error::NotFound("商品不存在或无权查看".into()))
    }

    async fn list_snapshot(&self, params: &ProductListParams, actor: &AuditActor) -> Result<ProductSnapshot> {
        let db = self.db.clone().ok_or_else(|| Error::Internal("商品中心未装配数据库".into()))?;
        let query = self.query.clone();
        let data_scope = self.data_scope.clone();
        let procurement = self.procurement.clone();
        let params = params.clone();
        let actor = actor.clone();
        db.client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    build_snapshot(db, query, data_scope, procurement, params, actor, executor).await
                })
            })
            .await
    }
}

struct ProductSnapshot {
    page: application_core::PageView<ProductView>,
    owner_options: Vec<FilterOption>,
    procurement_owner_options: Vec<FilterOption>,
    scope_version: String,
    policy_version: u64,
    organization_version: u64,
    as_of: String,
    no_scope: bool,
}

async fn build_snapshot(
    db: mongodb::Database,
    query: Arc<dyn CatalogSupplyQueryPort>,
    data_scope: Arc<dyn CatalogDataScopePort>,
    procurement: Arc<dyn ProductProcurementOwners>,
    params: ProductListParams,
    actor: AuditActor,
    executor: &mut dyn persistence_core::Executor,
) -> Result<ProductSnapshot> {
    let access = CatalogAccess::new(db.clone(), data_scope.clone());
    let (mut context, scope) = access.resolve(&actor, "list", executor).await?;
    let no_scope = !context.has_scope_rules();
    let mut filter = prepare_product_list(&params)?;
    filter.scope = Some(scope.clone());
    apply_org_filter(&mut filter, data_scope.as_ref(), &params, executor).await?;
    apply_procurement_filter(&db, &mut filter, &scope, procurement.as_ref(), &params, executor).await?;
    let page = query.product_page(&filter, executor).await?;
    let mut fingerprint = std::collections::hash_map::DefaultHasher::new();
    page.total.hash(&mut fingerprint);
    context.scope_version = format!("{}:{:x}", context.scope_version, fingerprint.finish());
    let mut owner_ids: Vec<String> = page.items.iter().map(|row| row.maintainer_user_id.clone()).collect();
    owner_ids.sort();
    owner_ids.dedup();
    let owner_options = db.accounts().filter_options(&owner_ids, executor).await?;
    let procurement_ids =
        params.procurement_owner_user_ids.as_ref().map(|ids| ids.as_slice().to_vec()).unwrap_or_default();
    let procurement_owner_options = db.accounts().filter_options(&procurement_ids, executor).await?;
    let view = product_page_view(page, &filter)?;
    Ok(ProductSnapshot {
        no_scope,
        page: view,
        owner_options,
        procurement_owner_options,
        scope_version: context.scope_version,
        policy_version: context.policy_version,
        organization_version: context.organization_version,
        as_of: context.as_of.as_utc().to_rfc3339(),
    })
}

async fn apply_org_filter(
    filter: &mut ProductFilter,
    data_scope: &dyn CatalogDataScopePort,
    params: &ProductListParams,
    executor: &mut dyn persistence_core::Executor,
) -> Result<()> {
    let Some(org_ids) = &params.org_unit_ids else {
        if params.include_descendants == Some(true) {
            return Err(Error::ValidationError("包含下级时必须提供组织筛选".into()));
        }
        return Ok(());
    };
    if params.include_descendants == Some(true) && org_ids.as_slice().is_empty() {
        return Err(Error::ValidationError("包含下级时必须提供组织筛选".into()));
    }
    let expanded = data_scope
        .expand_org_units(org_ids.as_slice(), params.include_descendants.unwrap_or(false), executor)
        .await?;
    filter.business_org_unit_ids = Some(expanded.into_iter().collect());
    Ok(())
}

async fn apply_procurement_filter(
    db: &mongodb::Database,
    filter: &mut ProductFilter,
    scope: &CatalogReadScope,
    procurement: &dyn ProductProcurementOwners,
    params: &ProductListParams,
    executor: &mut dyn persistence_core::Executor,
) -> Result<()> {
    let Some(owners) = &params.procurement_owner_user_ids else {
        return Ok(());
    };
    let authorized = if scope.is_company() {
        None
    } else {
        let ids = db.products().list_authorized_ids(scope, executor).await?;
        if ids.len() > 10_000 {
            return Err(Error::ValidationError("采购负责人筛选超过上限，请收窄组织或维护人条件".into()));
        }
        Some(ids)
    };
    let matched =
        procurement.matching_product_ids(authorized.as_deref(), owners.as_slice(), executor).await?;
    filter.ids = Some(matched);
    Ok(())
}

fn data_scope_changed(detail: &str) -> Error {
    Error::ConflictError(format!("DATA_SCOPE_CHANGED：{detail}"))
}
