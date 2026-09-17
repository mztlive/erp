//! 履约订单范围授权 adapter：调用身份域公共解析器，转换成履约 Port 事实。

use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;

use application_core::AuditActor;
use async_trait::async_trait;
use erp_core::common::time::Instant;
use erp_identity::SharedRbacService;
use erp_identity::access_control::{ResolvedScope, ScopeClause, ScopedObject};
use erp_identity::entity::organization::OrgTree;
use erp_identity::entity::organization_change::OrganizationState;
use erp_identity::repository::OrganizationRepository;
use erp_identity::service::access_control::consumers::registration;
use erp_identity::service::access_control::resolve::DataScopeService;
use erp_supply::service::supplier_fulfillment::{SupplierFulfillmentService, W26_BUSINESS_OBJECT_TYPE};
use erp_supply::{
    FulfillmentExceptionHandlerPort, FulfillmentOrderAccess, FulfillmentOrderDataScopePort,
    FulfillmentOrderResolvedClause, FulfillmentOrderResolvedScope, FulfillmentOrderScopeObject,
};
use erp_workflow::{WorkItem, WorkItemExt, WorkItemFilter, WorkItemRow, WorkItemStatus, WorkItemType};
use mongodb::Database;
use persistence_core::Executor;

/// 组合层履约订单范围 adapter，持有身份域解析所需依赖。
#[derive(Clone)]
pub struct MongoFulfillmentOrderDataScope {
    db: Database,
    rbac: SharedRbacService,
}

impl MongoFulfillmentOrderDataScope {
    /// 绑定身份数据库及现有 RBAC 实例。
    pub fn new(db: Database, rbac: SharedRbacService) -> Self {
        Self { db, rbac }
    }

    /// 包装为履约域可注入的共享 Port。
    pub fn shared(db: Database, rbac: SharedRbacService) -> Arc<dyn FulfillmentOrderDataScopePort> {
        Arc::new(Self::new(db, rbac))
    }
}

#[async_trait]
impl FulfillmentOrderDataScopePort for MongoFulfillmentOrderDataScope {
    fn allows(
        &self,
        scope: &FulfillmentOrderResolvedScope,
        object: &FulfillmentOrderScopeObject,
    ) -> erp_supply::Result<bool> {
        evaluate_object(scope, object)
    }

    async fn resolve(
        &self,
        actor: &AuditActor,
        action: &str,
        executor: &mut dyn Executor,
    ) -> erp_supply::Result<FulfillmentOrderResolvedScope> {
        let access = DataScopeService::new(self.db.clone(), self.rbac.clone())
            .resolve(actor, "supplier_fulfillment_order", action, executor)
            .await
            .map_err(map_identity_error)?;
        Ok(FulfillmentOrderResolvedScope {
            user_id: access.user_id,
            resource: access.resource,
            action: access.action,
            role_clauses: map_clauses(&access.scope.role_clauses)?,
            user_limit: access.scope.user_limit.as_ref().map(map_clause).transpose()?,
            policy_version: access.policy_version,
            organization_version: access.organizations.version,
            scope_version: access.scope_version,
            as_of: access.as_of,
        })
    }

    async fn expand_org_units(
        &self,
        org_unit_ids: &[String],
        include_descendants: bool,
        executor: &mut dyn Executor,
    ) -> erp_supply::Result<BTreeSet<String>> {
        let state = organization_state(&self.db, executor).await?;
        expand_org_units(&state, org_unit_ids, include_descendants)
    }

    async fn org_member_ids(
        &self,
        org_unit_ids: &BTreeSet<String>,
        at: Instant,
        executor: &mut dyn Executor,
    ) -> erp_supply::Result<Vec<String>> {
        let state = organization_state(&self.db, executor).await?;
        Ok(member_ids(&state, org_unit_ids, at))
    }

    async fn own_org(
        &self,
        user_id: &str,
        at: Instant,
        executor: &mut dyn Executor,
    ) -> erp_supply::Result<Option<String>> {
        let state = organization_state(&self.db, executor).await?;
        Ok(state.own_org(user_id, at).map_err(map_identity_error)?.map(str::to_string))
    }
}

/// 当前开放 W26 异常处理人 adapter。
#[derive(Clone)]
pub struct MongoFulfillmentExceptionHandlers {
    db: Database,
}

impl MongoFulfillmentExceptionHandlers {
    /// 绑定工作项集合。
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// 包装为履约域可注入的共享 Port。
    pub fn shared(db: Database) -> Arc<dyn FulfillmentExceptionHandlerPort> {
        Arc::new(Self::new(db))
    }
}

#[async_trait]
impl FulfillmentExceptionHandlerPort for MongoFulfillmentExceptionHandlers {
    async fn open_handler_user_ids(
        &self,
        order_ids: &[String],
        executor: &mut dyn Executor,
    ) -> erp_supply::Result<HashMap<String, String>> {
        let mut handlers = HashMap::new();
        for order_id in order_ids {
            if let Some(owner) = open_w26_owner(&self.db, order_id, executor).await? {
                handlers.entry(order_id.clone()).or_insert(owner);
            }
        }
        Ok(handlers)
    }

    async fn order_ids_for_handlers(
        &self,
        handler_user_ids: &[String],
        executor: &mut dyn Executor,
    ) -> erp_supply::Result<Vec<String>> {
        if handler_user_ids.is_empty() {
            return Ok(Vec::new());
        }
        let mut ids = scan_open_w26_order_ids(&self.db, handler_user_ids, executor).await?;
        ids.sort();
        ids.dedup();
        Ok(ids)
    }
}

async fn open_w26_owner(
    db: &Database,
    order_id: &str,
    executor: &mut dyn Executor,
) -> erp_supply::Result<Option<String>> {
    let items = db.work_items().list_active_by_object(W26_BUSINESS_OBJECT_TYPE, order_id, executor).await?;
    Ok(items.into_iter().find_map(w26_owner))
}

fn w26_owner(item: WorkItem) -> Option<String> {
    if !matches!(
        item.work_item_type,
        WorkItemType::IntegrationResultUnknown | WorkItemType::BusinessException
    ) {
        return None;
    }
    item.owner_user_id.filter(|id| !id.is_empty())
}

async fn scan_open_w26_order_ids(
    db: &Database,
    handler_user_ids: &[String],
    executor: &mut dyn Executor,
) -> erp_supply::Result<Vec<String>> {
    let filter = w26_handler_filter(handler_user_ids);
    let batch = std::num::NonZeroU32::new(100).expect("W26 扫描批次");
    let mut offset = 0;
    let mut ids = Vec::new();
    loop {
        let rows = db.work_items().scan_work_item_batch(&filter, offset, batch, executor).await?;
        let count = rows.len() as u64;
        ids.extend(rows.into_iter().filter_map(w26_row_order_id));
        if count < u64::from(batch.get()) {
            break;
        }
        offset = offset.saturating_add(count);
    }
    Ok(ids)
}

fn w26_handler_filter(handler_user_ids: &[String]) -> WorkItemFilter {
    WorkItemFilter {
        work_item_types: vec![WorkItemType::IntegrationResultUnknown, WorkItemType::BusinessException],
        statuses: vec![WorkItemStatus::Open],
        managed_owner_ids: Some(handler_user_ids.to_vec()),
        object_access_shapes: Some(vec![
            (WorkItemType::IntegrationResultUnknown, W26_BUSINESS_OBJECT_TYPE.to_string()),
            (WorkItemType::BusinessException, W26_BUSINESS_OBJECT_TYPE.to_string()),
        ]),
        page: 1,
        page_size: 100,
        sort_by: Some("created_at".into()),
        sort_ascending: true,
        ..WorkItemFilter::default()
    }
}

fn w26_row_order_id(row: WorkItemRow) -> Option<String> {
    (row.business_object_type == W26_BUSINESS_OBJECT_TYPE)
        .then_some(row.business_object_id)
        .filter(|id| !id.is_empty())
}

async fn organization_state(
    db: &Database,
    executor: &mut dyn Executor,
) -> erp_supply::Result<OrganizationState> {
    Ok(OrganizationRepository::new(db).state(executor).await?)
}

fn expand_org_units(
    state: &OrganizationState,
    org_ids: &[String],
    include_descendants: bool,
) -> erp_supply::Result<BTreeSet<String>> {
    let tree = OrgTree::new(&state.units).map_err(map_identity_error)?;
    let mut expanded = BTreeSet::new();
    for id in org_ids {
        expanded.extend(tree.expand(id, include_descendants).map_err(map_identity_error)?);
    }
    Ok(expanded)
}

fn member_ids(state: &OrganizationState, org_ids: &BTreeSet<String>, at: Instant) -> Vec<String> {
    let mut ids = state
        .memberships
        .iter()
        .filter(|membership| {
            !membership.base.is_deleted()
                && org_ids.contains(&membership.org_unit_id)
                && membership.validity.contains(at)
        })
        .map(|membership| membership.user_id.clone())
        .collect::<Vec<_>>();
    ids.sort();
    ids.dedup();
    ids
}

fn map_clauses(clauses: &[ScopeClause]) -> erp_supply::Result<Vec<FulfillmentOrderResolvedClause>> {
    clauses.iter().map(map_clause).collect()
}

fn map_clause(clause: &ScopeClause) -> erp_supply::Result<FulfillmentOrderResolvedClause> {
    if !clause.settlement_party_ids.is_empty() || !clause.warehouse_ids.is_empty() {
        return Err(erp_supply::Error::ValidationError("供应商履约订单范围不支持结算主体或仓库维度".into()));
    }
    Ok(FulfillmentOrderResolvedClause {
        company: clause.company,
        self_owned: clause.self_owned,
        collaborative: clause.collaborative,
        org_unit_ids: clause.org_unit_ids.iter().cloned().collect(),
    })
}

fn map_identity_error(error: erp_identity::Error) -> erp_supply::Error {
    match error {
        erp_identity::Error::Internal(payload) => erp_supply::Error::Internal(payload),
        erp_identity::Error::NotFound(payload) => erp_supply::Error::NotFound(payload),
        erp_identity::Error::ValidationError(payload) => erp_supply::Error::ValidationError(payload),
        erp_identity::Error::BusinessLogicError(payload) => erp_supply::Error::BusinessLogicError(payload),
        erp_identity::Error::ConflictError(payload) => erp_supply::Error::ConflictError(payload),
        erp_identity::Error::ReceiptDuplicate(payload) => erp_supply::Error::ReceiptDuplicate(payload),
        erp_identity::Error::TransientTransaction(payload) => {
            erp_supply::Error::TransientTransaction(payload)
        },
        erp_identity::Error::Forbidden(payload) => erp_supply::Error::Forbidden(payload),
        erp_identity::Error::Unauthenticated(payload) => erp_supply::Error::Unauthenticated(payload),
        erp_identity::Error::Logic(payload) => erp_supply::Error::Logic(payload),
        erp_identity::Error::Rbac(payload) => erp_supply::Error::Internal(payload),
        erp_identity::Error::OutcomeUnknown(payload) => erp_supply::Error::OutcomeUnknown(payload),
        erp_identity::Error::RepositoryError(payload) => erp_supply::Error::RepositoryError(payload),
    }
}

fn evaluate_object(
    scope: &FulfillmentOrderResolvedScope,
    object: &FulfillmentOrderScopeObject,
) -> erp_supply::Result<bool> {
    if scope.resource != "supplier_fulfillment_order" {
        return Err(erp_supply::Error::ValidationError("范围资源与消费方不一致".into()));
    }
    let consumer = registration(&scope.resource, &scope.action).map_err(map_identity_error)?;
    let resolved = ResolvedScope {
        role_clauses: scope.role_clauses.iter().map(public_clause).collect(),
        user_limit: scope.user_limit.as_ref().map(public_clause),
    };
    Ok(resolved.allows(
        &ScopedObject {
            owned: object.owned,
            collaborating: false,
            historical_read_participant: false,
            org_unit_id: object.org_unit_id.as_deref(),
            settlement_party_id: None,
            warehouse_id: None,
        },
        consumer.allows_history,
    ))
}

fn public_clause(clause: &FulfillmentOrderResolvedClause) -> ScopeClause {
    ScopeClause {
        company: clause.company,
        self_owned: clause.self_owned,
        collaborative: clause.collaborative,
        org_unit_ids: clause.org_unit_ids.iter().cloned().collect(),
        ..ScopeClause::default()
    }
}

/// 构造绑定身份数据库的履约订单访问器。
pub fn fulfillment_order_access(db: Database, rbac: SharedRbacService) -> FulfillmentOrderAccess {
    FulfillmentOrderAccess::new(db.clone(), MongoFulfillmentOrderDataScope::shared(db, rbac))
}

/// 构造已接入身份域公共解析器的履约服务。
pub fn scoped_fulfillment_service(db: Database, rbac: SharedRbacService) -> SupplierFulfillmentService {
    SupplierFulfillmentService::new(db.clone()).with_scope(
        MongoFulfillmentOrderDataScope::shared(db.clone(), rbac),
        MongoFulfillmentExceptionHandlers::shared(db),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fulfillment_adapter_rejects_unsupported_scope_dimensions() {
        let warehouse =
            ScopeClause { warehouse_ids: BTreeSet::from(["wh-1".into()]), ..ScopeClause::default() };
        match map_clause(&warehouse) {
            Err(erp_supply::Error::ValidationError(message)) => assert!(message.contains("仓库")),
            other => panic!("expected validation error, got {other:?}"),
        }
    }

    #[test]
    fn identity_forbidden_stays_forbidden() {
        match map_identity_error(erp_identity::Error::Forbidden("没有该资源动作权限".into())) {
            erp_supply::Error::Forbidden(message) => assert_eq!(message, "没有该资源动作权限"),
            other => panic!("expected forbidden, got {other:?}"),
        }
    }

    #[test]
    fn unwired_port_and_mismatched_resource_fail_closed() {
        let mut scope = FulfillmentOrderResolvedScope {
            user_id: "actor".into(),
            resource: "supplier_fulfillment_order".into(),
            action: "detail".into(),
            role_clauses: vec![FulfillmentOrderResolvedClause {
                company: true,
                ..FulfillmentOrderResolvedClause::default()
            }],
            user_limit: None,
            policy_version: 1,
            organization_version: 1,
            scope_version: "v1".into(),
            as_of: Instant::from_unix_secs(0),
        };
        let object = FulfillmentOrderScopeObject::default();
        assert!(erp_supply::FailClosedFulfillmentOrderDataScopePort.allows(&scope, &object).is_err());
        assert!(evaluate_object(&scope, &object).unwrap());
        scope.resource = "work_item".into();
        assert!(evaluate_object(&scope, &object).is_err());
    }
}

#[cfg(test)]
mod equivalence_tests {
    use erp_supply::{FulfillmentOrderReadScope, FulfillmentOrderScopeClause, fulfillment_order_scope};
    use serde_json::json;
    use test_support::matches_filter as matches;

    use super::*;

    fn clause(mask: u8) -> FulfillmentOrderResolvedClause {
        FulfillmentOrderResolvedClause {
            company: mask & 1 != 0,
            self_owned: mask & 2 != 0,
            collaborative: mask & 4 != 0,
            org_unit_ids: if mask & 8 != 0 { vec!["org-a".into()] } else { vec![] },
        }
    }

    fn scope_clause(mask: u8, actor: &str) -> FulfillmentOrderScopeClause {
        FulfillmentOrderScopeClause {
            company: mask & 1 != 0,
            owner_user_id: (mask & 2 != 0).then(|| actor.to_string()),
            business_org_unit_ids: if mask & 8 != 0 { vec!["org-a".into()] } else { vec![] },
        }
    }

    #[test]
    fn public_object_decision_matches_compiled_conditions() {
        let ids = (0..16).map(|i| format!("o-{i}")).collect::<Vec<_>>();
        for action in ["list", "detail", "submit"] {
            for role in 0..16 {
                for second in [0, 2, 8] {
                    for limit in -1..16 {
                        let access = FulfillmentOrderResolvedScope {
                            user_id: "actor".into(),
                            resource: "supplier_fulfillment_order".into(),
                            action: action.into(),
                            role_clauses: vec![clause(role), clause(second)],
                            user_limit: (limit >= 0).then(|| clause(limit as u8)),
                            policy_version: 1,
                            organization_version: 1,
                            scope_version: "v1".into(),
                            as_of: Instant::from_unix_secs(0),
                        };
                        let compiled = FulfillmentOrderReadScope {
                            roles: vec![scope_clause(role, "actor"), scope_clause(second, "actor")],
                            user_limit: (limit >= 0).then(|| scope_clause(limit as u8, "actor")),
                        };
                        assert_eq!(
                            fulfillment_order_scope(&access, "actor"),
                            compiled,
                            "action={action}, role={role}, second={second}, limit={limit}"
                        );
                        for (index, id) in ids.iter().enumerate() {
                            let object = FulfillmentOrderScopeObject {
                                owned: index & 1 != 0,
                                org_unit_id: Some(if index & 4 != 0 { "org-a" } else { "org-b" }.into()),
                            };
                            let owner = if object.owned { "actor" } else { "other" };
                            let org = object.org_unit_id.as_deref().unwrap();
                            let document = json!({
                                "id": id,
                                "follow_up_user_id": owner,
                                "business_org_unit_id": org
                            });
                            assert_eq!(
                                evaluate_object(&access, &object).unwrap(),
                                matches(&compiled.document(), &document),
                                "action={action}, role={role}, second={second}, limit={limit}, object={index}"
                            );
                        }
                    }
                }
            }
        }
    }
}
