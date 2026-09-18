//! 集成范围授权 adapter：调用身份域公共解析器，转换成集成 Port 事实。

use std::collections::BTreeSet;
use std::sync::Arc;

use application_core::AuditActor;
use async_trait::async_trait;
use erp_core::common::time::Instant;
use erp_identity::SharedRbacService;
use erp_identity::access_control::{ResolvedScope, ScopeClause, ScopedObject};
use erp_identity::service::access_control::consumers::registration;
use erp_identity::service::access_control::resolve::DataScopeService;
use erp_integration::{
    IntegrationDataScopePort, IntegrationOpsService, IntegrationResolvedClause, IntegrationResolvedScope,
    IntegrationScopeObject,
};
use mongodb::Database;
use persistence_core::Executor;

use super::identity_error::map_identity_error;
use super::scope_support::{expand_org_ids, load_organization_state, member_ids};

map_identity_error!(erp_integration);

/// 组合层集成范围 adapter，持有身份域解析所需依赖。
#[derive(Clone)]
pub struct MongoIntegrationDataScope {
    db: Database,
    rbac: SharedRbacService,
}

impl MongoIntegrationDataScope {
    /// 绑定身份数据库及现有 RBAC 实例。
    ///
    /// # 参数
    /// * `db` - 身份与组织集合所在数据库
    /// * `rbac` - 现有 RBAC 快照服务
    ///
    /// # 返回
    /// 返回未执行 I/O 的 adapter。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// 不得在构造时补公司范围或自行解释原始范围规则。
    pub fn new(db: Database, rbac: SharedRbacService) -> Self {
        Self { db, rbac }
    }

    /// 包装为集成域可注入的共享 Port。
    ///
    /// # 参数
    /// * `db` - 身份与组织集合所在数据库
    /// * `rbac` - 现有 RBAC 快照服务
    ///
    /// # 返回
    /// 返回集成范围 Port。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// 解析必须调用 DataScopeService；不得把身份实体交给集成域。
    pub fn shared(db: Database, rbac: SharedRbacService) -> Arc<dyn IntegrationDataScopePort> {
        Arc::new(Self::new(db, rbac))
    }
}

#[async_trait]
impl IntegrationDataScopePort for MongoIntegrationDataScope {
    fn allows(
        &self,
        scope: &IntegrationResolvedScope,
        object: &IntegrationScopeObject,
    ) -> erp_integration::Result<bool> {
        evaluate_object(scope, object)
    }

    async fn resolve(
        &self,
        actor: &AuditActor,
        resource: &str,
        action: &str,
        executor: &mut dyn Executor,
    ) -> erp_integration::Result<IntegrationResolvedScope> {
        ensure_integration_resource(resource)?;
        let access = DataScopeService::new(self.db.clone(), self.rbac.clone())
            .resolve(actor, resource, action, executor)
            .await
            .map_err(map_identity_error)?;
        map_access(access)
    }

    async fn expand_org_units(
        &self,
        org_unit_ids: &[String],
        include_descendants: bool,
        executor: &mut dyn Executor,
    ) -> erp_integration::Result<BTreeSet<String>> {
        let state = load_organization_state(&self.db, executor).await?;
        expand_org_ids(&state, org_unit_ids, include_descendants).map_err(map_identity_error)
    }

    async fn org_member_ids(
        &self,
        org_unit_ids: &BTreeSet<String>,
        at: Instant,
        executor: &mut dyn Executor,
    ) -> erp_integration::Result<Vec<String>> {
        let state = load_organization_state(&self.db, executor).await?;
        Ok(member_ids(&state, org_unit_ids, at))
    }

    async fn own_org(
        &self,
        user_id: &str,
        at: Instant,
        executor: &mut dyn Executor,
    ) -> erp_integration::Result<Option<String>> {
        let state = load_organization_state(&self.db, executor).await?;
        Ok(state.own_org(user_id, at).map_err(map_identity_error)?.map(str::to_string))
    }
}

/// 校验集成资源名；错误任务与差异共用同一 adapter。
fn ensure_integration_resource(resource: &str) -> erp_integration::Result<()> {
    if matches!(resource, "integration_error_task" | "reconciliation_difference") {
        return Ok(());
    }
    Err(erp_integration::Error::ValidationError("范围资源与消费方不一致".into()))
}

/// 将身份域已解析授权转换为集成 Port 事实。
fn map_access(
    access: erp_identity::service::access_control::resolve::AuthorizedDataScope,
) -> erp_integration::Result<IntegrationResolvedScope> {
    Ok(IntegrationResolvedScope {
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

/// 转换全部角色条款；任一不支持维度即失败。
fn map_clauses(clauses: &[ScopeClause]) -> erp_integration::Result<Vec<IntegrationResolvedClause>> {
    clauses.iter().map(map_clause).collect()
}

/// 将身份域条款转为集成已解析条款。
fn map_clause(clause: &ScopeClause) -> erp_integration::Result<IntegrationResolvedClause> {
    if !clause.settlement_party_ids.is_empty() || !clause.warehouse_ids.is_empty() {
        return Err(erp_integration::Error::ValidationError("集成范围不支持结算主体或仓库维度".into()));
    }
    Ok(IntegrationResolvedClause {
        company: clause.company,
        self_owned: clause.self_owned,
        org_unit_ids: clause.org_unit_ids.iter().cloned().collect(),
    })
}

/// 构造已接入公共解析器的集成服务。
///
/// # 参数
/// * `db` - 数据库
/// * `rbac` - 当前 RBAC 快照
///
/// # 返回
/// 返回可解析集成范围的服务。
///
/// # 错误
/// 无。
pub fn scoped_integration_ops_service(db: Database, rbac: SharedRbacService) -> IntegrationOpsService {
    IntegrationOpsService::with_scope(db.clone(), MongoIntegrationDataScope::shared(db, rbac))
}

/// 将本域已解析事实无损转回公共判定输入。
fn evaluate_object(
    scope: &IntegrationResolvedScope,
    object: &IntegrationScopeObject,
) -> erp_integration::Result<bool> {
    ensure_integration_resource(&scope.resource)?;
    let consumer = registration(&scope.resource, &scope.action).map_err(map_identity_error)?;
    let resolved = ResolvedScope {
        role_clauses: scope.role_clauses.iter().map(public_clause).collect(),
        user_limit: scope.user_limit.as_ref().map(public_clause),
    };
    Ok(resolved.allows(
        &ScopedObject {
            owned: object.owned,
            collaborating: false,
            historical_read_participant: object.historical_read_participant,
            org_unit_id: object.org_unit_id.as_deref(),
            settlement_party_id: None,
            warehouse_id: None,
        },
        consumer.allows_history,
    ))
}

/// 转换已解析条款，保留本人、组织及空集。
fn public_clause(clause: &IntegrationResolvedClause) -> ScopeClause {
    ScopeClause {
        company: clause.company,
        self_owned: clause.self_owned,
        collaborative: false,
        org_unit_ids: clause.org_unit_ids.iter().cloned().collect(),
        ..ScopeClause::default()
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    #[test]
    fn integration_adapter_rejects_unsupported_scope_dimensions() {
        let warehouse =
            ScopeClause { warehouse_ids: BTreeSet::from(["wh-1".into()]), ..ScopeClause::default() };
        assert!(matches!(map_clause(&warehouse), Err(erp_integration::Error::ValidationError(_))));
        let settlement = ScopeClause {
            settlement_party_ids: BTreeSet::from(["party-1".into()]),
            ..ScopeClause::default()
        };
        assert!(matches!(map_clause(&settlement), Err(erp_integration::Error::ValidationError(_))));
    }
}

#[cfg(test)]
mod equivalence_tests {
    use erp_integration::FailClosedIntegrationDataScopePort;
    use erp_integration::repository::{IntegrationReadScope, IntegrationScopeClause};
    use erp_integration::service::access::integration_scope;
    use serde_json::json;
    use test_support::matches_filter as matches;

    use super::*;

    fn clause(mask: u8) -> IntegrationResolvedClause {
        IntegrationResolvedClause {
            company: mask & 1 != 0,
            self_owned: mask & 2 != 0,
            org_unit_ids: if mask & 8 != 0 { vec!["org-a".into()] } else { vec![] },
        }
    }

    fn scope_clause(mask: u8, actor: &str) -> IntegrationScopeClause {
        IntegrationScopeClause {
            company: mask & 1 != 0,
            owner_user_id: (mask & 2 != 0).then(|| actor.to_string()),
            owner_org_unit_ids: if mask & 8 != 0 { vec!["org-a".into()] } else { vec![] },
        }
    }

    #[test]
    fn unwired_port_and_mismatched_resource_fail_closed() {
        let mut scope = IntegrationResolvedScope {
            user_id: "actor".into(),
            resource: "integration_error_task".into(),
            action: "list".into(),
            role_clauses: vec![clause(1)],
            user_limit: None,
            policy_version: 1,
            organization_version: 1,
            scope_version: "v1".into(),
            as_of: Instant::from_unix_secs(0),
        };
        let object = IntegrationScopeObject::default();
        assert!(FailClosedIntegrationDataScopePort.allows(&scope, &object).is_err());
        assert!(evaluate_object(&scope, &object).unwrap());
        scope.resource = "work_item".into();
        assert!(evaluate_object(&scope, &object).is_err());
    }

    #[test]
    fn public_object_decision_matches_compiled_conditions() {
        let ids = (0..16).map(|i| format!("o-{i}")).collect::<Vec<_>>();
        for action in ["list", "detail", "create"] {
            for role in 0..16 {
                for second in [0, 2, 8] {
                    for limit in -1..16 {
                        let access = IntegrationResolvedScope {
                            user_id: "actor".into(),
                            resource: "integration_error_task".into(),
                            action: action.into(),
                            role_clauses: vec![clause(role), clause(second)],
                            user_limit: (limit >= 0).then(|| clause(limit as u8)),
                            policy_version: 1,
                            organization_version: 1,
                            scope_version: "v1".into(),
                            as_of: Instant::from_unix_secs(0),
                        };
                        let compiled = IntegrationReadScope {
                            roles: vec![scope_clause(role, "actor"), scope_clause(second, "actor")],
                            user_limit: (limit >= 0).then(|| scope_clause(limit as u8, "actor")),
                        };
                        assert_eq!(
                            integration_scope(&access, "actor"),
                            compiled,
                            "action={action}, role={role}, second={second}, limit={limit}"
                        );
                        for (index, id) in ids.iter().enumerate() {
                            let object = IntegrationScopeObject {
                                owned: index & 1 != 0,
                                historical_read_participant: false,
                                org_unit_id: Some(if index & 4 != 0 { "org-a" } else { "org-b" }.into()),
                            };
                            let owner = if object.owned { "actor" } else { "other" };
                            let org = object.org_unit_id.as_deref().unwrap();
                            let document = json!({
                                "id": id,
                                "owner_user_id": owner,
                                "owner_org_unit_id": org
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
    fn empty_scope_matches_no_documents() {
        let compiled = IntegrationReadScope::default();
        assert!(compiled.is_empty());
        let document = json!({ "id": "o-1", "owner_user_id": "actor", "owner_org_unit_id": "org-a" });
        assert!(!matches(&compiled.document(), &document));
    }
}
