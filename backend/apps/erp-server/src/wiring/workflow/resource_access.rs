//! 任务和审批入口独立证明功能权限，数据资格由统一读取授权决定。
use application_core::AuditActor;
use erp_identity::access_control::ScopedObject;
use erp_identity::entity::resource_access::query::ResourceTargets;
use erp_identity::entity::resource_access::{ResourceKind, SourceKey};
use erp_identity::service::resource_access::ResourceAccessService;
use erp_identity::{Permission, SharedRbacService};
use erp_procurement::repository::PurchaseOrderExt;
use erp_resource_grants::{GrantReadiness, ResourceGrantProcess};
use erp_sales::repository::SalesOrderExt;
use erp_workflow::ports::OrderTaskSource;
use erp_workflow::repository::prelude::*;
use erp_workflow::{DocumentRegistryExt, ErrorCode};
use mongodb::Database;
use persistence_core::Executor;

use crate::wiring::errors::{Error, Result};

/// 当前入口与精确业务来源必须同时成立，读取授权不包含任务分配资格。
pub(super) async fn readable(
    db: &Database,
    rbac: &SharedRbacService,
    actor: &AuditActor,
    source: &OrderTaskSource,
    entry: &str,
    executor: &mut dyn Executor,
) -> Result<bool> {
    let (kind, id) = match source {
        OrderTaskSource::Sales(id) => (ResourceKind::SalesOrder, id),
        OrderTaskSource::Purchase(id) => (ResourceKind::PurchaseOrder, id),
    };
    Ok(readable_batch(db, rbac, actor, kind, std::slice::from_ref(id), entry, executor).await?.contains(id))
}

/// 每种来源只解析一次权限，实际目标与历史参与均在同一事务批量读取。
/// # 参数
/// db、rbac、actor 为认证设施；kind、ids 为有界目标；entry 为真实入口；executor 为原事务。
/// # 返回
/// 本批有读取权的目标 ID。
/// # 错误
/// 身份、功能、来源或数据库失败时拒绝。
pub(super) async fn readable_batch(
    db: &Database,
    rbac: &SharedRbacService,
    actor: &AuditActor,
    kind: ResourceKind,
    ids: &[String],
    entry: &str,
    executor: &mut dyn Executor,
) -> Result<Vec<String>> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let access = ResourceAccessService::reader(db.clone(), rbac.clone())
        .resolve_targets(
            actor,
            &Permission::parse(entry)?,
            kind,
            ResourceTargets::new(&ids.iter().map(String::as_str).collect::<Vec<_>>())?,
            executor,
        )
        .await?;
    let history = db.document_participants().document_ids_in(actor.id(), ids, executor).await?;
    let facts = source_facts(db, kind, ids, executor).await?;
    Ok(facts
        .into_iter()
        .filter_map(|(id, owner, org)| {
            access
                .allows(
                    &id,
                    &ScopedObject {
                        owned: owner == actor.id(),
                        collaborating: false,
                        historical_read_participant: history.contains(&id),
                        org_unit_id: Some(&org),
                        settlement_party_id: None,
                        warehouse_id: None,
                    },
                )
                .then_some(id)
        })
        .collect())
}

/// 由领域仓储读取本批订单的真实责任字段。
async fn source_facts(
    db: &Database,
    kind: ResourceKind,
    ids: &[String],
    executor: &mut dyn Executor,
) -> Result<Vec<(String, String, String)>> {
    match kind {
        ResourceKind::SalesOrder => Ok(db
            .sales_orders()
            .list_active_by_ids(ids, executor)
            .await?
            .into_iter()
            .map(|order| (order.base.id, order.sales_owner_user_id, order.business_org_unit_id))
            .collect()),
        ResourceKind::PurchaseOrder => db
            .purchase_orders()
            .list_active_by_ids(ids, executor)
            .await?
            .into_iter()
            .map(|order| {
                let owner = order.current_owner_user_id()?.to_owned();
                Ok((order.base.id, owner, order.business_org_unit_id))
            })
            .collect(),
    }
}

/// 实际办理审批时检查当前来源；合法分配事件未应用时只返回刷新状态。
pub(super) async fn require_approval(
    db: &Database,
    rbac: &SharedRbacService,
    actor: &AuditActor,
    source: &OrderTaskSource,
    instance_id: &str,
    executor: &mut dyn Executor,
) -> Result<()> {
    if readable(db, rbac, actor, source, "approval_instance:decide", executor).await? {
        return Ok(());
    }
    let key = SourceKey {
        rule: match source {
            OrderTaskSource::Sales(_) => "sales_order_approval_assigned",
            OrderTaskSource::Purchase(_) => "purchase_order_approval_assigned",
        }
        .into(),
        business_type: "approval_instance".into(),
        business_id: instance_id.into(),
        recipient: actor.id().into(),
    };
    if ResourceGrantProcess::new(db.clone(), rbac.clone()).source_readiness(&key, executor).await?
        == GrantReadiness::NotReady
    {
        return Err(Error::Coded(ErrorCode::ResourceAuthorizationNotReady));
    }
    Err(Error::Forbidden("没有审批单据的读取资格".into()))
}

#[cfg(test)]
mod tests {
    use erp_procurement::repository::purchase_order::scope::{PurchaseReadScope, PurchaseScopeClause};
    use erp_sales::repository::sales_order::scope::{SalesReadScope, SalesScopeClause};
    use mongodb::bson::doc;
    use serde_json::json;
    use test_support::matches_filter;

    #[test]
    fn explicit_sales_read_is_union_outside_personal_limit_and_inside_other_required_scope() {
        let mut scope = SalesReadScope {
            roles: vec![SalesScopeClause { owner_user_id: Some("owner".into()), ..Default::default() }],
            user_limit: Some(SalesScopeClause::default()),
            grant_lookup: vec![doc! { "$lookup": {} }],
            ..Default::default()
        };
        let granted = json!({"id":"S001", "sales_owner_user_id":"other", "customer_id":"same", "__resource_grant":[{"id":"g"}]});
        let other =
            json!({"id":"S002", "sales_owner_user_id":"other", "customer_id":"same", "__resource_grant":[]});
        assert!(matches_filter(&scope.document(), &granted));
        assert!(!matches_filter(&scope.document(), &other));
        assert!(!scope.is_empty());
        scope.required_scopes.push(SalesReadScope::default());
        assert!(!matches_filter(&scope.document(), &granted));
        assert!(scope.is_empty());
        scope.required_scopes.clear();
        scope.grant_lookup.clear();
        assert!(!matches_filter(&scope.document(), &granted));
    }

    #[test]
    fn purchase_read_keeps_owned_rows_and_revocation_removes_only_extra_read() {
        let scope = PurchaseReadScope {
            roles: vec![PurchaseScopeClause { owner_user_id: Some("actor".into()), ..Default::default() }],
            grant_lookup: vec![doc! { "$lookup": {} }],
            ..Default::default()
        };
        for grants in [json!([]), json!([{"id":"a"}]), json!([{"id":"a"},{"id":"b"}])] {
            assert!(matches_filter(
                &scope.document(),
                &json!({"id":"own", "owner_user_id":"actor", "__resource_grant":grants})
            ));
        }
        assert!(matches_filter(
            &scope.document(),
            &json!({"id":"extra", "owner_user_id":"other", "__resource_grant":[{"id":"remaining"}]})
        ));
        assert!(!matches_filter(
            &scope.document(),
            &json!({"id":"extra", "owner_user_id":"other", "__resource_grant":[]})
        ));
    }
}

#[cfg(test)]
mod history_scope_tests {
    use erp_sales::repository::sales_order::scope::{SalesReadScope, SalesScopeClause};
    use erp_workflow::repository::document_registry::participant_access;
    use mongodb::bson::doc;
    use serde_json::json;
    use test_support::matches_filter;
    #[test]
    fn history_lookup_grants_only_matched_orders_and_keeps_personal_limit() {
        let mut scope =
            SalesReadScope { history_lookup: participant_access::lookup("reader"), ..Default::default() };
        let historical =
            json!({"id":"S001", "__document_participant":[{"_id":"p"}], "business_org_unit_id":"dept"});
        let unrelated = json!({"id":"S002", "__document_participant":[]});
        assert!(!scope.is_empty());
        assert!(matches_filter(&scope.document(), &historical));
        assert!(!matches_filter(&scope.document(), &unrelated));
        scope.user_limit = Some(SalesScopeClause::default());
        assert!(!matches_filter(&scope.document(), &historical));
        scope.grant_lookup = vec![doc! {"$lookup":{}}];
        let explicit = json!({"id":"S001", "__resource_grant":[{"id":"g"}]});
        assert!(matches_filter(&scope.document(), &explicit));
        assert!(!matches_filter(&scope.document(), &unrelated));
        let stages = scope.lookup_after(doc! {"id":"S001"});
        assert_eq!(stages[0], doc! {"$match":{"id":"S001"}});
        let lookup = stages[1].get_document("$lookup").unwrap();
        assert_eq!(lookup.get_document("let").unwrap(), &doc! {"document":"$id"});
        assert_eq!(lookup.get_array("pipeline").unwrap()[1], doc! {"$limit":1}.into());
        assert!(scope.historical_order_ids.is_empty());
    }
}
