//! 授权范围内的供应商履约订单列表；跟进人与异常处理人分列筛选。

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use application_core::{AuditActor, FilterOption, FilteredPage};
use erp_identity::AccessControlExt;
use erp_supply::dto::supplier_fulfillment::{
    FulfillmentOrderListQuery, SupplierFulfillmentOrderListParams, SupplierFulfillmentOrderListView,
    SupplierFulfillmentOrderView,
};
use erp_supply::service::supplier_fulfillment::query::fulfillment_order_filter;
use persistence_core::Transactional;
use validator::Validate;

use super::SupplierFulfillmentProcess;
use crate::{Error, Result};

impl SupplierFulfillmentProcess {
    /// 分页查询授权范围内的供应商履约订单。
    ///
    /// # 参数
    /// * `params` - 查询参数
    /// * `actor` - 已认证操作人
    ///
    /// # 返回
    /// 返回分页、跟进人候选、处理人候选与范围元信息。
    ///
    /// # 错误
    /// 缺范围版本的后续页、范围变化、筛选非法或仓储失败时拒绝。
    pub async fn supplier_fulfillment_order_list(
        &self,
        params: &SupplierFulfillmentOrderListParams,
        actor: &AuditActor,
    ) -> Result<SupplierFulfillmentOrderListView> {
        params.validate()?;
        let query = params.normalized()?;
        ensure_page(query.paging.page, params.scope_version.as_deref())?;
        let snapshot = self.list_snapshot(&query, actor).await?;
        if params.scope_version.as_deref().is_some_and(|value| value != snapshot.scope_version) {
            return Err(data_scope_changed("数据范围已变化，请从第一页刷新"));
        }
        Ok(to_list_view(snapshot))
    }

    async fn list_snapshot(
        &self,
        query: &FulfillmentOrderListQuery,
        actor: &AuditActor,
    ) -> Result<ListSnapshot> {
        let db = self.db.clone();
        let domain = self.domain();
        let query = query.clone();
        let actor = actor.clone();
        db.client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move { build_snapshot(&db, &domain, &query, &actor, executor).await })
            })
            .await
    }
}

struct ListSnapshot {
    page: application_core::PageView<SupplierFulfillmentOrderView>,
    owner_options: Vec<FilterOption>,
    handler_options: Vec<FilterOption>,
    scope_version: String,
    policy_version: u64,
    organization_version: u64,
    as_of: String,
    no_scope: bool,
}

async fn build_snapshot(
    db: &mongodb::Database,
    domain: &erp_supply::service::supplier_fulfillment::SupplierFulfillmentService,
    query: &FulfillmentOrderListQuery,
    actor: &AuditActor,
    executor: &mut dyn persistence_core::Executor,
) -> Result<ListSnapshot> {
    let access = domain.access();
    let (mut context, scope) = access.resolve(actor, "list", executor).await?;
    let no_scope = !context.has_scope_rules();
    let mut filter = fulfillment_order_filter(query);
    filter.scope = Some(scope);
    apply_org_filter(&access, query, &mut filter, executor).await?;
    apply_handler_filter(domain, query, &mut filter, executor).await?;
    let mut page = domain.search_fulfillment_orders(filter, executor).await?;
    attach_handlers(domain, &mut page.items, executor).await?;
    fingerprint(&mut context.scope_version, page.total);
    let owner_ids = unique_ids(page.items.iter().map(|row| row.follow_up_user_id.clone()));
    let handler_ids = unique_ids(page.items.iter().filter_map(|row| row.handler_user_id.clone()));
    let owner_options = db.accounts().filter_options(&owner_ids, executor).await?;
    let handler_options = db.accounts().filter_options(&handler_ids, executor).await?;
    Ok(ListSnapshot {
        no_scope,
        page,
        owner_options,
        handler_options,
        scope_version: context.scope_version,
        policy_version: context.policy_version,
        organization_version: context.organization_version,
        as_of: context.as_of.as_utc().to_rfc3339(),
    })
}

async fn apply_org_filter(
    access: &erp_supply::FulfillmentOrderAccess,
    query: &FulfillmentOrderListQuery,
    filter: &mut erp_supply::service::supplier_fulfillment::query::FulfillmentOrderFilter,
    executor: &mut dyn persistence_core::Executor,
) -> Result<()> {
    let Some(org_ids) = &query.org_unit_ids else {
        if query.include_descendants == Some(true) {
            return Err(Error::ValidationError("包含下级时必须提供组织筛选".into()));
        }
        return Ok(());
    };
    if query.include_descendants == Some(true) && org_ids.as_slice().is_empty() {
        return Err(Error::ValidationError("包含下级时必须提供组织筛选".into()));
    }
    let expanded = access
        .expand_org_units(org_ids.as_slice(), query.include_descendants.unwrap_or(false), executor)
        .await?;
    filter.business_org_unit_ids = Some(expanded.into_iter().collect());
    Ok(())
}

async fn apply_handler_filter(
    domain: &erp_supply::service::supplier_fulfillment::SupplierFulfillmentService,
    query: &FulfillmentOrderListQuery,
    filter: &mut erp_supply::service::supplier_fulfillment::query::FulfillmentOrderFilter,
    executor: &mut dyn persistence_core::Executor,
) -> Result<()> {
    let Some(handlers) = &query.handler_user_ids else {
        return Ok(());
    };
    let ids = domain.handlers().order_ids_for_handlers(handlers.as_slice(), executor).await?;
    filter.handler_order_ids = Some(ids);
    Ok(())
}

async fn attach_handlers(
    domain: &erp_supply::service::supplier_fulfillment::SupplierFulfillmentService,
    items: &mut [SupplierFulfillmentOrderView],
    executor: &mut dyn persistence_core::Executor,
) -> Result<()> {
    let order_ids: Vec<String> = items.iter().map(|row| row.id.clone()).collect();
    let handlers = domain.handlers().open_handler_user_ids(&order_ids, executor).await?;
    for item in items {
        item.handler_user_id = handlers.get(&item.id).cloned();
    }
    Ok(())
}

fn fingerprint(scope_version: &mut String, total: i64) {
    let mut hasher = DefaultHasher::new();
    total.hash(&mut hasher);
    *scope_version = format!("{}:{:x}", scope_version, hasher.finish());
}

fn unique_ids(ids: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut values: Vec<String> = ids.into_iter().filter(|id| !id.is_empty()).collect();
    values.sort();
    values.dedup();
    values
}

fn to_list_view(snapshot: ListSnapshot) -> SupplierFulfillmentOrderListView {
    SupplierFulfillmentOrderListView {
        scope_version: snapshot.scope_version,
        policy_version: snapshot.policy_version,
        organization_version: snapshot.organization_version,
        as_of: snapshot.as_of,
        empty_reason: snapshot.no_scope.then_some("no_scope"),
        scope_summary: "API 供应商订单跟进人及其业务组织范围；异常处理人为当前开放 W26 任务",
        handler_options: snapshot.handler_options,
        data: FilteredPage {
            owner_options: snapshot.owner_options,
            ownership_basis: "fulfillment_follow_up",
            page: snapshot.page,
        },
    }
}

fn data_scope_changed(detail: &str) -> Error {
    Error::ConflictError(format!("DATA_SCOPE_CHANGED：{detail}"))
}

fn ensure_page(page: u64, version: Option<&str>) -> Result<()> {
    if page > 1 && version.is_none_or(str::is_empty) {
        return Err(data_scope_changed("请从第一页刷新后继续查询"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn later_pages_without_scope_version_are_rejected() {
        assert!(ensure_page(1, None).is_ok());
        match ensure_page(2, None) {
            Err(Error::ConflictError(message)) => assert!(message.starts_with("DATA_SCOPE_CHANGED：")),
            other => panic!("expected DATA_SCOPE_CHANGED, got {other:?}"),
        }
    }
}
