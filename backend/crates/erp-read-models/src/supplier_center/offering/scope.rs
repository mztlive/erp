//! 供给列表的一致授权快照；范围与业务版本跨页携带。

use std::collections::HashSet;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use application_core::{AuditActor, OwnershipPage};
use erp_core::ids::SupplierOfferingId;
use erp_identity::AccessControlExt;
use erp_identity::repository::prelude::*;
use erp_supply::repository::supplier_offering::SupplierOfferingFilter;
use erp_supply::repository::supplier_offering::procurement::ProcurementOfferingRepositoryExt;
use erp_supply::{OfferingAccess, OfferingDataScopePort, SupplierOfferingExt};
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
        crate::support::ensure_deep_page(params.page.unwrap_or(1), params.scope_version.as_deref())?;
        let snapshot = self.list_snapshot(params, actor).await?;
        if params.scope_version.as_deref().is_some_and(|value| value != snapshot.scope_version) {
            return Err(crate::support::data_scope_changed("数据范围已变化，请从第一页刷新"));
        }
        Ok(SupplierOfferingListView {
            data: OwnershipPage { ownership_basis: "offering_maintainer", page: snapshot.page },

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
    let filter = apply_procurement_filter(&db, &query, procurement.as_ref(), &params, executor).await?;
    let bundle = match filter {
        Some(filter) => {
            super::SupplierOfferingReadRepository::new(&db)
                .load_offering_list_page_by_filter(&filter, executor)
                .await?
        },
        None => super::repository_page(&db, &query, executor).await?,
    };
    let mut fingerprint = std::collections::hash_map::DefaultHasher::new();
    bundle.page.total.hash(&mut fingerprint);
    context.scope_version = format!("{}:{:x}", context.scope_version, fingerprint.finish());
    let mut view = super::page_view(bundle, &query)?;
    let ids = view.items.iter().map(|row| row.maintainer_user_id.clone()).collect::<Vec<_>>();
    let names = db.accounts().names_by_ids(&ids, executor).await?;
    for row in &mut view.items {
        row.maintainer_user_name = names.get(&row.maintainer_user_id).cloned();
    }
    Ok(OfferingSnapshot {
        no_scope,
        page: view,

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

/// 先用完整查询条件读取身份，再解析负责人，禁止对页面先切片后过滤。
async fn apply_procurement_filter(
    db: &mongodb::Database,
    query: &crate::supplier_center::repository::offering::SupplierOfferingListQuery,
    procurement: &dyn OfferingProcurementOwners,
    params: &SupplierOfferingListParams,
    executor: &mut dyn persistence_core::Executor,
) -> Result<Option<SupplierOfferingFilter>> {
    let Some(owners) = &params.procurement_owner_user_ids else {
        return Ok(None);
    };
    let mut filter =
        super::SupplierOfferingReadRepository::new(db).resolve_list_filter(query, executor).await?;
    let candidates = db.supplier_offerings().procurement_candidate_ids(&filter, executor).await?;
    apply_candidate_filter(&mut filter, &candidates, procurement, owners.as_slice(), executor).await?;
    Ok(Some(filter))
}

/// 解析已筛选候选并收窄身份，复用原过滤的授权、搜索、状态及页面字段。
async fn apply_candidate_filter(
    filter: &mut SupplierOfferingFilter,
    candidates: &[String],
    procurement: &dyn OfferingProcurementOwners,
    owner_ids: &[String],
    executor: &mut dyn persistence_core::Executor,
) -> Result<()> {
    ensure_authorized_count(candidates.len())?;
    let matched = procurement.matching_offering_ids(Some(candidates), owner_ids, executor).await?;
    let matched_ids = matched.into_iter().map(SupplierOfferingId::new).collect();
    filter.offering_ids = Some(intersect_ids(filter.offering_ids.take(), matched_ids));
    Ok(())
}

/// 保留可供状态与其它既有身份筛选，仅收窄到采购责任命中项；空命中保持空集。
fn intersect_ids(
    existing: Option<Vec<SupplierOfferingId>>,
    matched: Vec<SupplierOfferingId>,
) -> Vec<SupplierOfferingId> {
    let Some(existing) = existing else {
        return matched;
    };
    let matched = matched.into_iter().collect::<HashSet<_>>();
    existing.into_iter().filter(|id| matched.contains(id)).collect()
}

#[cfg(test)]
mod tests {
    use erp_core::ids::{SkuId, SupplierAccountId};
    use erp_supply::OfferingReadScope;
    use erp_supply::entity::supplier_offering::{OfferingSourceType, OfferingStatus};
    use persistence_core::{NoTransaction, QueryFilter};

    use super::*;
    use crate::supplier_center::offering::procurement::MapOfferingProcurementOwners;

    #[test]
    /// 最终负责人身份与已解析的可供状态/既有身份求交，不能放宽候选或丢掉空筛选。
    fn offering_procurement_ids_preserve_existing_filter_intersection() {
        let ids = |values: &[&str]| values.iter().map(|id| SupplierOfferingId::new(*id)).collect();
        assert_eq!(
            intersect_ids(Some(ids(&["off-a", "off-b"])), ids(&["off-b", "outside"])),
            ids(&["off-b"])
        );
        assert!(intersect_ids(Some(ids(&["off-a"])), Vec::new()).is_empty());
        assert!(intersect_ids(Some(Vec::new()), ids(&["outside"])).is_empty());
        assert_eq!(intersect_ids(None, ids(&["off-b"])), ids(&["off-b"]));
    }

    #[tokio::test]
    /// 实际候选编排仅收窄身份，其余已解析的关键词、编号、授权和深页条件全部保留。
    async fn offering_candidate_filter_keeps_complete_query_and_page_fields() {
        let mut filter = SupplierOfferingFilter {
            offering_ids: Some(vec![SupplierOfferingId::new("off-a"), SupplierOfferingId::new("off-b")]),
            sku_ids: Some(vec![SkuId::new("sku")]),
            keyword_sku_ids: Some(vec![SkuId::new("keyword-sku")]),
            supplier_id: Some(SupplierAccountId::new("supplier")),
            supplier_sku_code: Some("礼盒.*".into()),
            source_type: Some(OfferingSourceType::Excel),
            status: Some(OfferingStatus::Active),
            scope: Some(OfferingReadScope::default()),
            maintainer_user_ids: Some(vec!["maintainer".into()]),
            business_org_unit_ids: Some(vec!["org".into()]),
            page: 7,
            page_size: 1,
            sort_by: Some("supplier_sku_code".into()),
            sort_ascending: true,
            ..Default::default()
        };
        let mut expected = filter.clone();
        expected.offering_ids = Some(vec![SupplierOfferingId::new("off-b")]);
        let port = MapOfferingProcurementOwners {
            owners: [("off-b".into(), "buyer".into()), ("outside".into(), "buyer".into())].into(),
        };
        apply_candidate_filter(
            &mut filter,
            &["off-a".into(), "off-b".into()],
            &port,
            &["buyer".into()],
            &mut NoTransaction,
        )
        .await
        .unwrap();
        assert_eq!(filter.to_doc(), expected.to_doc());
        assert_eq!((filter.page, filter.page_size, filter.sort_ascending), (7, 1, true));
        assert_eq!(filter.sort_by.as_deref(), Some("supplier_sku_code"));
        apply_candidate_filter(&mut filter, &[], &port, &["buyer".into()], &mut NoTransaction).await.unwrap();
        assert_eq!(filter.offering_ids, Some(Vec::new()));
        assert!(
            apply_candidate_filter(
                &mut filter,
                &vec!["id".into(); 10001],
                &port,
                &["buyer".into()],
                &mut NoTransaction
            )
            .await
            .is_err()
        );
    }

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
