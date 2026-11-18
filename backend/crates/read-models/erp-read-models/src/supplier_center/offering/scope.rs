//! 供给列表的一致授权快照；范围与业务版本跨页携带。

use std::collections::HashSet;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use application_core::{AuditActor, OwnershipPage};
use erp_core::ids::SupplierOfferingId;
use erp_identity::AccessControlExt;
use erp_identity::repository::prelude::*;
use erp_permissions::supplier_offering as supplier_offering_permissions;
use erp_supply::repository::supplier_offering::SupplierOfferingFilter;
use erp_supply::repository::supplier_offering::procurement::ProcurementOfferingRepositoryExt;
use erp_supply::{OfferingAccess, SupplierOfferingExt};
use persistence_core::{Executor, Transactional};

use super::procurement::OfferingProcurementOwners;
use super::{SupplierOfferingListParams, SupplierOfferingListView, SupplierOfferingReadService};
use crate::catalog_center::procurement::ensure_authorized_count;
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
        let service = Self::new(self.db.clone(), self.access.clone(), self.procurement.clone());
        let (params, actor) = (params.clone(), actor.clone());
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move { service.list_with(&params, &actor, executor).await })
            })
            .await
    }

    /// 在调用方快照内读取完整供给分页并完成展示补齐与原凭证交付检查。
    ///
    /// # 参数
    /// * `params` - 原供给筛选、排序、分页及跨页范围版本。
    /// * `actor` - 当前已认证账号。
    /// * `executor` - 调用方活动快照，资格、候选、分页、计数及补齐共用它。
    ///
    /// # 返回
    /// 返回原供给列表视图，保留分页成员、顺序、总数及范围元信息。
    ///
    /// # 错误
    /// 非快照、无列表资格、范围版本变化、非法参数或任一读取失败时拒绝。
    pub async fn list_with(
        &self,
        params: &SupplierOfferingListParams,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<SupplierOfferingListView> {
        if !executor.boundary().is_some_and(|boundary| boundary.is_snapshot()) {
            return Err(Error::ValidationError("暂时无法查看供给列表，请从第一页重新查询".into()));
        }
        crate::support::ensure_deep_page(params.page.unwrap_or(1), params.scope_version.as_deref())?;

        let snapshot = build_snapshot(
            self.db.clone(),
            self.access.clone(),
            self.procurement.clone(),
            params.clone(),
            actor.clone(),
            executor,
        )
        .await?;

        if params.scope_version.as_deref().is_some_and(|value| value != snapshot.scope_version) {
            return Err(crate::support::data_scope_changed("数据范围已变化，请从第一页刷新"));
        }
        Ok(SupplierOfferingListView {
            data: OwnershipPage { ownership_basis: "offering_maintainer", page: snapshot.page },
            scope_version: snapshot.scope_version,
            policy_version: snapshot.policy_version,
            organization_version: snapshot.organization_version,
            as_of: snapshot.as_of,
            empty_reason: None,
            scope_summary: "供给按岗位权限共享；维护人及采购负责人筛选为额外收窄",
        })
    }
}

struct OfferingSnapshot {
    page: application_core::PageView<super::dto::SupplierOfferingView>,

    scope_version: String,
    policy_version: u64,
    organization_version: u64,
    as_of: String,
}

async fn build_snapshot(
    db: mongodb::Database,
    access: OfferingAccess,
    procurement: Arc<dyn OfferingProcurementOwners>,
    params: SupplierOfferingListParams,
    actor: AuditActor,
    executor: &mut dyn persistence_core::Executor,
) -> Result<OfferingSnapshot> {
    let mut context = access.resolve(&actor, &supplier_offering_permissions::LIST, executor).await?;
    context.ensure_current(executor)?;
    let mut query = super::prepare_list_query(&params)?;
    query.scope = Some(context.clone());

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

    let mut view = super::page_view(bundle, &query);
    let ids = view.items.iter().map(|row| row.maintainer_user_id.clone()).collect::<Vec<_>>();
    let names = db.accounts().names_by_ids(&ids, executor).await?;
    for row in &mut view.items {
        row.maintainer_user_name = names.get(&row.maintainer_user_id).cloned();
    }

    context.validate(executor).await?;
    Ok(OfferingSnapshot {
        page: view,

        policy_version: context.policy_version(),
        organization_version: context.organization_version(),
        as_of: context.as_of().as_utc().to_rfc3339(),
        scope_version: context.scope_version,
    })
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
    let matched = procurement.matching_offering_ids(candidates, owner_ids, executor).await?;
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
    use erp_core::AccountKind;
    use erp_core::ids::{SkuId, SupplierAccountId};
    use erp_identity::testing::unavailable_authorization;
    use erp_supply::entity::supplier_offering::{OfferingSourceType, OfferingStatus};
    use persistence_core::testing::SnapshotExecutor;
    use persistence_core::{NoTransaction, QueryFilter, ReadExecutor};

    use super::*;
    use crate::supplier_center::offering::MongoOfferingProcurementOwners;
    use crate::supplier_center::offering::procurement::MapOfferingProcurementOwners;

    #[tokio::test]
    async fn offering_list_with_rejects_non_snapshot_and_missing_qualification_before_business_reads() {
        let db = mongodb::Client::with_uri_str("mongodb://127.0.0.1:1/?serverSelectionTimeoutMS=1")
            .await
            .unwrap()
            .database("offering_list_snapshot_authorization");
        let service = SupplierOfferingReadService::new(
            db.clone(),
            OfferingAccess::new(db.clone(), unavailable_authorization()),
            MongoOfferingProcurementOwners::shared(db),
        );
        let params = SupplierOfferingListParams::default();
        let actor = AuditActor::new("actor".into(), "actor".into(), AccountKind::Admin);

        let result = service.list_with(&params, &actor, &mut ReadExecutor::default()).await;
        assert!(
            matches!(result, Err(Error::ValidationError(reason)) if reason == "暂时无法查看供给列表，请从第一页重新查询")
        );

        let result = service.list_with(&params, &actor, &mut SnapshotExecutor::default()).await;
        assert!(matches!(result, Err(Error::Internal(reason)) if reason == "测试授权设施不可用"));
    }

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
            scope: None,
            maintainer_user_ids: Some(vec!["maintainer".into()]),
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
            .matching_offering_ids(&["off-a".into(), "off-b".into()], &["buyer-1".into()], &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(matched, vec!["off-a".to_string()]);
        let none = port
            .matching_offering_ids(&["off-b".into()], &["buyer-1".into()], &mut NoTransaction)
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
    fn offering_list_rejects_removed_organization_filters() {
        for value in [
            serde_json::json!({"org_unit_ids": "org-a"}),
            serde_json::json!({"include_descendants": true}),
            serde_json::json!({"include_descendants": false}),
        ] {
            let error = serde_json::from_value::<SupplierOfferingListParams>(value).unwrap_err();
            assert!(error.to_string().contains("unknown field"));
        }
    }
}
