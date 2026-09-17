//! 结算范围授权 adapter：调用身份域公共解析器，转换成结算 Port 事实。

use std::collections::BTreeSet;
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
use erp_supply::service::supplier_settlement::SupplierSettlementService;
use erp_supply::{
    SettlementDataScopePort, SettlementResolvedClause, SettlementResolvedScope, SettlementScopeObject,
};
use mongodb::Database;
use persistence_core::Executor;

use crate::supply_settlement::SupplierSettlementProcess;

/// 组合层结算范围 adapter，持有身份域解析所需依赖。
#[derive(Clone)]
pub struct MongoSettlementDataScope {
    db: Database,
    rbac: SharedRbacService,
}

impl MongoSettlementDataScope {
    /// 绑定身份数据库及现有 RBAC 实例。
    ///
    /// # 参数
    /// * `db` - 身份与组织集合所在数据库
    /// * `rbac` - 现有 RBAC 快照服务
    ///
    /// # 返回
    /// 返回未执行 I/O 的 adapter。
    pub fn new(db: Database, rbac: SharedRbacService) -> Self {
        Self { db, rbac }
    }

    /// 包装为结算域可注入的共享 Port。
    ///
    /// # 参数
    /// * `db` - 身份与组织集合所在数据库
    /// * `rbac` - 现有 RBAC 快照服务
    ///
    /// # 返回
    /// 返回结算范围 Port。
    pub fn shared(db: Database, rbac: SharedRbacService) -> Arc<dyn SettlementDataScopePort> {
        Arc::new(Self::new(db, rbac))
    }
}

#[async_trait]
impl SettlementDataScopePort for MongoSettlementDataScope {
    fn allows(
        &self,
        scope: &SettlementResolvedScope,
        object: &SettlementScopeObject,
    ) -> erp_supply::Result<bool> {
        evaluate_object(scope, object)
    }

    async fn resolve(
        &self,
        actor: &AuditActor,
        action: &str,
        executor: &mut dyn Executor,
    ) -> erp_supply::Result<SettlementResolvedScope> {
        let access = DataScopeService::new(self.db.clone(), self.rbac.clone())
            .resolve(actor, "supplier_settlement_statement", action, executor)
            .await
            .map_err(map_identity_error)?;
        Ok(SettlementResolvedScope {
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

fn map_clauses(clauses: &[ScopeClause]) -> erp_supply::Result<Vec<SettlementResolvedClause>> {
    clauses.iter().map(map_clause).collect()
}

fn map_clause(clause: &ScopeClause) -> erp_supply::Result<SettlementResolvedClause> {
    if !clause.settlement_party_ids.is_empty() || !clause.warehouse_ids.is_empty() {
        return Err(erp_supply::Error::ValidationError("结算范围不支持结算主体或仓库维度".into()));
    }
    Ok(SettlementResolvedClause {
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
    scope: &SettlementResolvedScope,
    object: &SettlementScopeObject,
) -> erp_supply::Result<bool> {
    if scope.resource != "supplier_settlement_statement" {
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

fn public_clause(clause: &SettlementResolvedClause) -> ScopeClause {
    ScopeClause {
        company: clause.company,
        self_owned: clause.self_owned,
        collaborative: clause.collaborative,
        org_unit_ids: clause.org_unit_ids.iter().cloned().collect(),
        ..ScopeClause::default()
    }
}

/// 构造绑定身份数据库的结算访问器。
///
/// # 参数
/// * `db` - 结算与身份集合所在数据库
/// * `rbac` - 现有 RBAC 快照服务
///
/// # 返回
/// 返回未缓存授权的结算访问器。
pub fn settlement_access(db: Database, rbac: SharedRbacService) -> erp_supply::SettlementAccess {
    erp_supply::SettlementAccess::new(db.clone(), MongoSettlementDataScope::shared(db, rbac))
}

/// 构造已接入公共解析器的结算服务。
///
/// # 参数
/// * `db` - 结算与身份集合所在数据库
/// * `rbac` - 现有 RBAC 快照服务
///
/// # 返回
/// 返回可解析结算范围的服务。
pub fn scoped_settlement_service(db: Database, rbac: SharedRbacService) -> SupplierSettlementService {
    SupplierSettlementService::new(db.clone()).with_data_scope(MongoSettlementDataScope::shared(db, rbac))
}

/// 构造已接入公共解析器的结算流程。
///
/// # 参数
/// * `db` - 结算与身份集合所在数据库
/// * `rbac` - 现有 RBAC 快照服务
///
/// # 返回
/// 返回可解析结算范围的跨域流程。
pub fn scoped_settlement_process(db: Database, rbac: SharedRbacService) -> SupplierSettlementProcess {
    SupplierSettlementProcess::new(db.clone())
        .with_scope(MongoSettlementDataScope::shared(db.clone(), rbac.clone()), rbac)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    #[test]
    fn settlement_adapter_rejects_unsupported_scope_dimensions() {
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
}

#[cfg(test)]
mod equivalence_tests {
    use erp_supply::{
        FailClosedSettlementDataScopePort, SettlementReadScope, SettlementScopeClause, settlement_scope,
    };
    use serde_json::json;
    use test_support::matches_filter as matches;

    use super::*;

    fn clause(mask: u8) -> SettlementResolvedClause {
        SettlementResolvedClause {
            company: mask & 1 != 0,
            self_owned: mask & 2 != 0,
            collaborative: mask & 4 != 0,
            org_unit_ids: if mask & 8 != 0 { vec!["org-a".into()] } else { vec![] },
        }
    }

    fn scope_clause(mask: u8, actor: &str) -> SettlementScopeClause {
        SettlementScopeClause {
            company: mask & 1 != 0,
            owner_user_id: (mask & 2 != 0).then(|| actor.to_string()),
            business_org_unit_ids: if mask & 8 != 0 { vec!["org-a".into()] } else { vec![] },
        }
    }

    #[test]
    fn unwired_port_and_mismatched_resource_fail_closed() {
        let mut scope = SettlementResolvedScope {
            user_id: "actor".into(),
            resource: "supplier_settlement_statement".into(),
            action: "detail".into(),
            role_clauses: vec![clause(1)],
            user_limit: None,
            policy_version: 1,
            organization_version: 1,
            scope_version: "v1".into(),
            as_of: Instant::from_unix_secs(0),
        };
        let object = SettlementScopeObject::default();
        assert!(FailClosedSettlementDataScopePort.allows(&scope, &object).is_err());
        assert!(evaluate_object(&scope, &object).unwrap());
        scope.resource = "work_item".into();
        assert!(evaluate_object(&scope, &object).is_err());
    }

    #[test]
    fn public_object_decision_matches_compiled_conditions() {
        let ids = (0..16).map(|i| format!("o-{i}")).collect::<Vec<_>>();
        for action in ["list", "detail", "create", "update", "submit", "confirm"] {
            for role in 0..16 {
                for second in [0, 2, 8] {
                    for limit in -1..16 {
                        let access = SettlementResolvedScope {
                            user_id: "actor".into(),
                            resource: "supplier_settlement_statement".into(),
                            action: action.into(),
                            role_clauses: vec![clause(role), clause(second)],
                            user_limit: (limit >= 0).then(|| clause(limit as u8)),
                            policy_version: 1,
                            organization_version: 1,
                            scope_version: "v1".into(),
                            as_of: Instant::from_unix_secs(0),
                        };
                        let compiled = SettlementReadScope {
                            roles: vec![scope_clause(role, "actor"), scope_clause(second, "actor")],
                            user_limit: (limit >= 0).then(|| scope_clause(limit as u8, "actor")),
                        };
                        assert_eq!(
                            settlement_scope(&access, "actor"),
                            compiled,
                            "action={action}, role={role}, second={second}, limit={limit}"
                        );
                        for (index, id) in ids.iter().enumerate() {
                            let object = SettlementScopeObject {
                                owned: index & 1 != 0,
                                org_unit_id: Some(if index & 4 != 0 { "org-a" } else { "org-b" }.into()),
                            };
                            let owner = if object.owned { "actor" } else { "other" };
                            let org = object.org_unit_id.as_deref().unwrap();
                            let document = json!({
                                "id": id,
                                "prepared_by": owner,
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

    #[test]
    fn empty_scope_and_three_role_filters_do_not_expand_visibility() {
        let empty = SettlementReadScope::default();
        assert!(empty.is_empty());
        assert!(!empty.allows_object("owner", "org-a"));
        let owner =
            SettlementScopeClause { owner_user_id: Some("owner".into()), ..SettlementScopeClause::default() };
        assert!(owner.allows("owner", "org-a"));
        assert!(!owner.allows("handler", "org-a"));
        assert!(!owner.allows("operator", "org-a"));
        let list_empty = SettlementResolvedScope {
            user_id: "actor".into(),
            resource: "supplier_settlement_statement".into(),
            action: "list".into(),
            role_clauses: vec![],
            user_limit: None,
            policy_version: 1,
            organization_version: 1,
            scope_version: "v1".into(),
            as_of: Instant::from_unix_secs(0),
        };
        let confirm = SettlementResolvedScope { action: "confirm".into(), ..list_empty.clone() };
        assert!(!list_empty.has_scope_rules());
        assert!(!confirm.has_scope_rules());
        assert_ne!(list_empty.action, confirm.action);
    }
}
