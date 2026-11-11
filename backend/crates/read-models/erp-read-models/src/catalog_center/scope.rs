//! 商品列表与详情的一致授权快照；范围与业务版本跨页携带。

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use application_core::{AuditActor, OwnershipPage};
use erp_catalog::ports::supply::CatalogSupplyQueryPort;
use erp_catalog::service::catalog::{prepare_product_list, product_page_view};
use erp_catalog::{CatalogAccess, ProductFilter, ProductListParams, ProductListView, ProductView};
use erp_identity::AccessControlExt;
use erp_identity::repository::prelude::*;
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
    /// 返回分页与范围元信息；人员候选由独立目录提供。
    ///
    /// # 错误
    /// 缺范围版本的后续页、范围变化、筛选非法或仓储失败时拒绝。
    pub async fn product_list(
        &self,
        params: &ProductListParams,
        actor: &AuditActor,
    ) -> Result<ProductListView> {
        crate::support::ensure_deep_page(params.page.unwrap_or(1), params.scope_version.as_deref())?;
        let snapshot = self.list_snapshot(params, actor).await?;
        if params.scope_version.as_deref().is_some_and(|value| value != snapshot.scope_version) {
            return Err(crate::support::data_scope_changed("数据范围已变化，请从第一页刷新"));
        }
        Ok(ProductListView {
            data: OwnershipPage { ownership_basis: "product_maintainer", page: snapshot.page },

            scope_version: snapshot.scope_version,
            policy_version: snapshot.policy_version,
            organization_version: snapshot.organization_version,
            as_of: snapshot.as_of,
            empty_reason: None,
            scope_summary: "商品按岗位权限共享；维护人及采购负责人筛选为额外收窄",
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
        let access = self.access.clone();
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

    /// 在同一事务装载列表快照。
    async fn list_snapshot(&self, params: &ProductListParams, actor: &AuditActor) -> Result<ProductSnapshot> {
        let db = self.db.clone();
        let query = self.query.clone();
        let access = self.access.clone();
        let procurement = self.procurement.clone();
        let params = params.clone();
        let actor = actor.clone();
        db.client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    build_snapshot(db, query, access, procurement, params, actor, executor).await
                })
            })
            .await
    }
}

struct ProductSnapshot {
    page: application_core::PageView<ProductView>,

    scope_version: String,
    policy_version: u64,
    organization_version: u64,
    as_of: String,
}

/// 在同一授权快照中查询商品，并按已授权行批量补齐维护人姓名。
async fn build_snapshot(
    db: mongodb::Database,
    query: Arc<dyn CatalogSupplyQueryPort>,
    access: CatalogAccess,
    procurement: Arc<dyn ProductProcurementOwners>,
    params: ProductListParams,
    actor: AuditActor,
    executor: &mut dyn persistence_core::Executor,
) -> Result<ProductSnapshot> {
    let mut context = access.resolve(&actor, "list", executor).await?;
    context.validate(executor).await?;
    let mut filter = prepare_product_list(&params)?;
    filter.scope = Some(context.clone());
    apply_org_filter(&mut filter, &access, &params, executor).await?;
    apply_procurement_filter(query.as_ref(), &mut filter, procurement.as_ref(), &params, executor).await?;
    let page = query.product_page(&filter, executor).await?;
    let mut fingerprint = std::collections::hash_map::DefaultHasher::new();
    page.total.hash(&mut fingerprint);
    context.scope_version = format!("{}:{:x}", context.scope_version, fingerprint.finish());
    let mut view = product_page_view(page, &filter)?;
    let ids = view.items.iter().map(|row| row.maintainer_user_id.clone()).collect::<Vec<_>>();
    let names = db.accounts().names_by_ids(&ids, executor).await?;
    apply_maintainer_names(&mut view.items, &names);
    context.validate(executor).await?;
    Ok(ProductSnapshot {
        page: view,

        policy_version: context.policy_version(),
        organization_version: context.organization_version(),
        as_of: context.as_of().as_utc().to_rfc3339(),
        scope_version: context.scope_version,
    })
}

/// 按维护人身份匹配展示名；缺失姓名保持空值，不将内部 ID 当作姓名。
///
/// # 参数
/// * `rows` - 待补维护人姓名的商品行
/// * `names` - 账号身份到展示名
///
/// # 返回
/// 无返回值。按 `maintainer_user_id` 写入 `maintainer_user_name`；映射没有该身份时为 `None`。
///
/// # 错误
/// 不返回错误。
pub(super) fn apply_maintainer_names(rows: &mut [ProductView], names: &HashMap<String, String>) {
    for row in rows {
        row.maintainer_user_name = names.get(&row.maintainer_user_id).cloned();
    }
}

/// 包含下级却没有组织时返回 `ValidationError`，不得展开成全部组织。
async fn apply_org_filter(
    filter: &mut ProductFilter,
    access: &CatalogAccess,
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
    let expanded = access
        .expand_org_units(org_ids.as_slice(), params.include_descendants.unwrap_or(false), executor)
        .await?;
    filter.business_org_unit_ids = Some(expanded.into_iter().collect());
    Ok(())
}

/// 先按完整查询读取候选身份，再在候选内解析采购责任，最终列表仍按全部条件分页。
///
/// # 参数
/// * `query` - 商品候选查询
/// * `filter` - 已含范围与组织条件的商品过滤；命中后写入 `ids`
/// * `procurement` - 采购负责人规则解析
/// * `params` - 列表参数；`procurement_owner_user_ids` 为 `None` 时不改过滤
/// * `executor` - 与授权相同的执行器
///
/// # 返回
/// `procurement_owner_user_ids` 为 `None` 时不改 `filter`。否则把命中商品写入 `filter.ids`。
///
/// # 错误
/// 候选超过 10000 时返回 `ValidationError`；候选读取或责任解析失败时返回对应错误。
pub(super) async fn apply_procurement_filter(
    query: &dyn CatalogSupplyQueryPort,
    filter: &mut ProductFilter,
    procurement: &dyn ProductProcurementOwners,
    params: &ProductListParams,
    executor: &mut dyn persistence_core::Executor,
) -> Result<()> {
    let Some(owners) = &params.procurement_owner_user_ids else {
        return Ok(());
    };
    let candidates = query.product_candidate_ids(filter, executor).await?;
    if candidates.len() > 10_000 {
        return Err(Error::ValidationError("采购负责人筛选超过上限，请收窄组织或维护人条件".into()));
    }
    let matched = procurement.matching_product_ids(Some(&candidates), owners.as_slice(), executor).await?;
    filter.ids = Some(matched);
    Ok(())
}
