//! 授权范围内的供应商履约订单列表；跟进人与异常处理人分列筛选。
//! 行姓名在授权查询之后按账号事实写入，不从候选标签回填。

use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use application_core::{AuditActor, OwnershipPage};
use erp_identity::AccessControlExt;
use erp_identity::repository::prelude::*;
use erp_supply::dto::supplier_fulfillment::{
    FulfillmentOrderListQuery, SupplierFulfillmentOrderListParams, SupplierFulfillmentOrderListView,
    SupplierFulfillmentOrderView,
};
use erp_supply::dto::supplier_fulfillment_scope::SupplierFulfillmentOrderListItem;
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
    /// 返回分页、行内跟进人与处理人姓名及范围元信息。
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
    page: application_core::PageView<SupplierFulfillmentOrderListItem>,

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
    let names = account_display_names(db, &page.items, executor).await?;
    Ok(ListSnapshot {
        no_scope,
        page: application_core::PageView {
            items: named_rows(page.items, &names),
            total: page.total,
            page: page.page,
            page_size: page.page_size,
        },

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

async fn account_display_names(
    db: &mongodb::Database,
    rows: &[SupplierFulfillmentOrderView],
    executor: &mut dyn persistence_core::Executor,
) -> Result<HashMap<String, String>> {
    let mut ids = unique_ids(rows.iter().map(|row| row.follow_up_user_id.clone()));
    ids.extend(unique_ids(rows.iter().filter_map(|row| row.handler_user_id.clone())));
    ids.sort();
    ids.dedup();
    db.accounts().names_by_ids(&ids, executor).await.map_err(Error::from)
}

/// 把已授权行上的账号姓名写入列表项。缺失姓名保持空，不用 ID 或另一角色顶替。
fn named_rows(
    rows: Vec<SupplierFulfillmentOrderView>,
    names: &HashMap<String, String>,
) -> Vec<SupplierFulfillmentOrderListItem> {
    rows.into_iter()
        .map(|order| {
            let follow_up_user_name = stored_name(names, &order.follow_up_user_id);
            let handler_user_name = order.handler_user_id.as_deref().and_then(|id| stored_name(names, id));
            SupplierFulfillmentOrderListItem { order, follow_up_user_name, handler_user_name }
        })
        .collect()
}

/// 只返回账号姓名。账号不存在或姓名为空白时返回 `None`，禁止回退成 ID。
fn stored_name(names: &HashMap<String, String>, id: &str) -> Option<String> {
    let trimmed = id.trim();
    if trimmed.is_empty() {
        return None;
    }
    names.get(trimmed).map(|name| name.trim().to_string()).filter(|name| !name.is_empty())
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

        data: OwnershipPage { ownership_basis: "fulfillment_follow_up", page: snapshot.page },
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
    use erp_supply::entity::supplier_fulfillment::{CancelStatus, FulfillmentStatus, RefundStatus};

    use super::*;

    #[test]
    fn later_pages_without_scope_version_are_rejected() {
        assert!(ensure_page(1, None).is_ok());
        match ensure_page(2, None) {
            Err(Error::ConflictError(message)) => assert!(message.starts_with("DATA_SCOPE_CHANGED：")),
            other => panic!("expected DATA_SCOPE_CHANGED, got {other:?}"),
        }
    }

    #[test]
    fn row_names_are_account_names_not_candidate_labels_or_ids() {
        let mut names = HashMap::new();
        names.insert("buyer-1".to_string(), " 跟进甲 ".to_string());
        names.insert("handler-1".to_string(), "处理乙".to_string());
        let items =
            named_rows(vec![order_view("buyer-1", Some("handler-1")), order_view("missing", None)], &names);
        let view = to_list_view(ListSnapshot {
            page: application_core::PageView { items, total: 2, page: 1, page_size: 20 },
            scope_version: "v".to_string(),
            policy_version: 1,
            organization_version: 1,
            as_of: "2026-09-23T00:00:00Z".to_string(),
            no_scope: false,
        });
        let json = serde_json::to_value(&view).expect("list view serializes");
        assert_eq!(json["items"][0]["follow_up_user_id"], "buyer-1");
        assert_eq!(json["items"][0]["follow_up_user_name"], "跟进甲");
        assert_eq!(json["items"][0]["handler_user_id"], "handler-1");
        assert_eq!(json["items"][0]["handler_user_name"], "处理乙");
        assert_eq!(json["items"][1]["follow_up_user_id"], "missing");
        assert!(json["items"][1].get("follow_up_user_name").is_none());
        assert!(json["items"][1].get("handler_user_id").is_none());
        assert!(json["items"][1].get("handler_user_name").is_none());
        assert_eq!(json["ownership_basis"], "fulfillment_follow_up");
        assert!(json.get("owner_options").is_none());
        assert!(json.get("handler_options").is_none());
    }

    #[test]
    fn blank_or_absent_handler_does_not_copy_follow_up_name() {
        let mut names = HashMap::new();
        names.insert("buyer-1".to_string(), "跟进甲".to_string());
        names.insert("blank".to_string(), "   ".to_string());
        let items =
            named_rows(vec![order_view("buyer-1", None), order_view("blank", Some("reviewer-1"))], &names);
        assert_eq!(items[0].follow_up_user_name.as_deref(), Some("跟进甲"));
        assert_eq!(items[0].handler_user_name, None);
        assert_eq!(items[1].follow_up_user_name, None);
        assert_eq!(items[1].handler_user_name, None);
        assert_ne!(items[1].handler_user_name.as_deref(), Some("跟进甲"));
    }

    fn order_view(follow_up_user_id: &str, handler_user_id: Option<&str>) -> SupplierFulfillmentOrderView {
        SupplierFulfillmentOrderView {
            id: "order-1".to_string(),
            fulfillment_order_no: "FO-1".to_string(),
            supplier_id: "supplier-1".to_string(),
            connection_id: "connection-1".to_string(),
            split_no: 1,
            fulfillment_status: FulfillmentStatus::Exception,
            cancel_status: CancelStatus::None,
            refund_status: RefundStatus::None,
            external_order_no: None,
            submitted_at: None,
            accepted_at: None,
            completed_at: None,
            follow_up_user_id: follow_up_user_id.to_string(),
            business_org_unit_id: "org-1".to_string(),
            handler_user_id: handler_user_id.map(str::to_string),
            version: 1,
            created_at: 1,
        }
    }
}
