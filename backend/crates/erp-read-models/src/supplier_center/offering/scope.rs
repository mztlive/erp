//! 供给列表的一致授权快照；范围与业务版本跨页携带。

use std::hash::{Hash, Hasher};
use std::sync::Arc;

use application_core::{AuditActor, FilterOption, FilteredPage};
use erp_identity::AccessControlExt;
use erp_supply::{OfferingAccess, OfferingDataScopePort, OfferingReadScope, SupplierOfferingExt};
use persistence_core::Transactional;

use super::procurement::{OfferingProcurementOwners, ensure_authorized_count};
use super::{SupplierOfferingListParams, SupplierOfferingListView, SupplierOfferingReadService};
use crate::{Error, Result};

impl SupplierOfferingReadService {
    /// 分页查询授权范围内的供给列表。
    ///
    /// # 参数
    /// * `params` - 供给筛选与范围参数
    /// * `actor` - 已认证操作人
    ///
    /// # 返回
    /// 返回分页、维护人候选、采购负责人候选与范围元信息。
    ///
    /// # 错误
    /// 缺范围版本的后续页、范围变化、筛选非法或仓储失败时拒绝。
    pub async fn list(
        &self,
        params: &SupplierOfferingListParams,
        actor: &AuditActor,
    ) -> Result<SupplierOfferingListView> {
        if params.page.unwrap_or(1) > 1 && params.scope_version.as_deref().is_none_or(str::is_empty) {
            return Err(data_scope_changed("请从第一页刷新后继续查询"));
        }
        let snapshot = self.list_snapshot(params, actor).await?;
        if params.scope_version.as_deref().is_some_and(|value| value != snapshot.scope_version) {
            return Err(data_scope_changed("数据范围已变化，请从第一页刷新"));
        }
        Ok(SupplierOfferingListView {
            data: FilteredPage {
                owner_options: snapshot.owner_options,
                ownership_basis: "offering_maintainer",
                page: snapshot.page,
            },
            procurement_owner_options: snapshot.procurement_owner_options,
            scope_version: snapshot.scope_version,
            policy_version: snapshot.policy_version,
            organization_version: snapshot.organization_version,
            as_of: snapshot.as_of,
            empty_reason: snapshot.no_scope.then_some("no_scope"),
            scope_summary: "供给维护人及其业务组织范围；采购负责人筛选为额外收窄",
        })
    }

    async fn list_snapshot(
        &self,
        params: &SupplierOfferingListParams,
        actor: &AuditActor,
    ) -> Result<OfferingSnapshot> {
        let db = self.db.clone();
        let data_scope = self.data_scope.clone();
        let procurement = self.procurement.clone();
        let params = params.clone();
        let actor = actor.clone();
        db.client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(
                    async move { build_snapshot(db, data_scope, procurement, params, actor, executor).await },
                )
            })
            .await
    }
}

struct OfferingSnapshot {
    page: application_core::PageView<super::dto::SupplierOfferingView>,
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
    data_scope: Arc<dyn OfferingDataScopePort>,
    procurement: Arc<dyn OfferingProcurementOwners>,
    params: SupplierOfferingListParams,
    actor: AuditActor,
    executor: &mut dyn persistence_core::Executor,
) -> Result<OfferingSnapshot> {
    let access = OfferingAccess::new(db.clone(), data_scope.clone());
    let (mut context, scope) = access.resolve(&actor, "list", executor).await?;
    let no_scope = !context.has_scope_rules();
    let mut query = super::prepare_list_query(&params)?;
    query.scope = Some(scope.clone());
    apply_org_filter(&mut query, data_scope.as_ref(), &params, executor).await?;
    apply_procurement_filter(&db, &mut query, &scope, procurement.as_ref(), &params, executor).await?;
    let bundle = super::repository_page(&db, &query, executor).await?;
    let mut fingerprint = std::collections::hash_map::DefaultHasher::new();
    bundle.page.total.hash(&mut fingerprint);
    context.scope_version = format!("{}:{:x}", context.scope_version, fingerprint.finish());
    let mut owner_ids: Vec<String> =
        bundle.page.items.iter().map(|row| row.maintainer_user_id.clone()).collect();
    owner_ids.sort();
    owner_ids.dedup();
    let owner_options = db.accounts().filter_options(&owner_ids, executor).await?;
    let procurement_ids =
        params.procurement_owner_user_ids.as_ref().map(|ids| ids.as_slice().to_vec()).unwrap_or_default();
    let procurement_owner_options = db.accounts().filter_options(&procurement_ids, executor).await?;
    let view = super::page_view(bundle, &query)?;
    Ok(OfferingSnapshot {
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
    query: &mut crate::supplier_center::repository::offering::SupplierOfferingListQuery,
    data_scope: &dyn OfferingDataScopePort,
    params: &SupplierOfferingListParams,
    executor: &mut dyn persistence_core::Executor,
) -> Result<()> {
    let Some((org_ids, include_descendants)) = requested_org_expansion(params)? else {
        return Ok(());
    };
    let expanded = data_scope.expand_org_units(&org_ids, include_descendants, executor).await?;
    query.business_org_unit_ids = Some(expanded.into_iter().collect());
    Ok(())
}

/// 解析组织筛选；包含下级时必须提供组织 ID，只收窄授权结果。
fn requested_org_expansion(params: &SupplierOfferingListParams) -> Result<Option<(Vec<String>, bool)>> {
    let include = params.include_descendants.unwrap_or(false);
    match &params.org_unit_ids {
        None if include => Err(Error::ValidationError("包含下级时必须提供组织筛选".into())),
        None => Ok(None),
        Some(ids) if include && ids.as_slice().is_empty() => {
            Err(Error::ValidationError("包含下级时必须提供组织筛选".into()))
        },
        Some(ids) => Ok(Some((ids.as_slice().to_vec(), include))),
    }
}

async fn apply_procurement_filter(
    db: &mongodb::Database,
    query: &mut crate::supplier_center::repository::offering::SupplierOfferingListQuery,
    scope: &OfferingReadScope,
    procurement: &dyn OfferingProcurementOwners,
    params: &SupplierOfferingListParams,
    executor: &mut dyn persistence_core::Executor,
) -> Result<()> {
    let Some(owners) = &params.procurement_owner_user_ids else {
        return Ok(());
    };
    let authorized = if scope.is_company() {
        None
    } else {
        let ids = db.supplier_offerings().list_authorized_ids(scope, executor).await?;
        ensure_authorized_count(ids.len())?;
        Some(ids)
    };
    let matched =
        procurement.matching_offering_ids(authorized.as_deref(), owners.as_slice(), executor).await?;
    let matched_ids: Vec<erp_core::ids::SupplierOfferingId> =
        matched.into_iter().map(erp_core::ids::SupplierOfferingId::new).collect();
    query.offering_ids = Some(intersect_ids(query.offering_ids.take(), matched_ids));
    Ok(())
}

fn intersect_ids(
    existing: Option<Vec<erp_core::ids::SupplierOfferingId>>,
    matched: Vec<erp_core::ids::SupplierOfferingId>,
) -> Vec<erp_core::ids::SupplierOfferingId> {
    let Some(existing) = existing else {
        return matched;
    };
    existing.into_iter().filter(|id| matched.iter().any(|other| other == id)).collect()
}

fn data_scope_changed(detail: &str) -> Error {
    Error::ConflictError(format!("DATA_SCOPE_CHANGED：{detail}"))
}

#[cfg(test)]
mod tests {
    use persistence_core::NoTransaction;

    use super::*;
    use crate::supplier_center::offering::procurement::MapOfferingProcurementOwners;

    #[tokio::test]
    async fn procurement_owner_filter_does_not_expand_maintainer_authorization() {
        let port = MapOfferingProcurementOwners {
            owners: [("off-a".into(), "buyer-1".into()), ("off-b".into(), "buyer-2".into())].into(),
        };
        let matched = port
            .matching_offering_ids(
                Some(&["off-a".into(), "off-b".into()]),
                &["buyer-1".into()],
                &mut NoTransaction,
            )
            .await
            .unwrap();
        assert_eq!(matched, vec!["off-a".to_string()]);
        let none = port
            .matching_offering_ids(Some(&["off-b".into()]), &["buyer-1".into()], &mut NoTransaction)
            .await
            .unwrap();
        assert!(none.is_empty());
    }

    #[test]
    fn offering_list_rejects_unknown_owner_alias() {
        assert!(
            serde_json::from_value::<SupplierOfferingListParams>(serde_json::json!({"owner": "张三"}))
                .is_err()
        );
        assert!(
            serde_json::from_value::<SupplierOfferingListParams>(serde_json::json!({
                "created_by_user_ids": "user-1"
            }))
            .is_err()
        );
    }

    #[test]
    fn include_descendants_requires_org_units() {
        let missing = SupplierOfferingListParams {
            include_descendants: Some(true),
            ..SupplierOfferingListParams::default()
        };
        match requested_org_expansion(&missing) {
            Err(Error::ValidationError(message)) => assert!(message.contains("组织筛选")),
            other => panic!("expected org filter, got {other:?}"),
        }
        let params: SupplierOfferingListParams = serde_json::from_value(serde_json::json!({
            "org_unit_ids": "org-a",
            "include_descendants": true
        }))
        .unwrap();
        let (ids, include) = requested_org_expansion(&params).unwrap().unwrap();
        assert_eq!(ids, vec!["org-a".to_string()]);
        assert!(include);
    }
}
