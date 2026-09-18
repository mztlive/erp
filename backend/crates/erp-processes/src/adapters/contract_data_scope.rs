//! 合同范围授权 adapter：调用身份域公共解析器，转换成合同 Port 事实。

use std::collections::BTreeSet;
use std::sync::Arc;

use application_core::AuditActor;
use async_trait::async_trait;
use erp_contract::ports::ContractScopeObject;
use erp_contract::{ContractDataScopePort, ContractResolvedClause, ContractResolvedScope};
use erp_core::common::time::Instant;
use erp_identity::SharedRbacService;
use erp_identity::access_control::{ResolvedScope, ScopeClause, ScopedObject};
use erp_identity::service::access_control::consumers::registration;
use erp_identity::service::access_control::resolve::DataScopeService;
use mongodb::Database;
use persistence_core::Executor;

use super::identity_error::map_identity_error;
use super::scope_support::{expand_org_ids, load_organization_state, member_ids};

map_identity_error!(erp_contract);

/// 组合层合同范围 adapter，持有身份域解析所需依赖。
#[derive(Clone)]
pub struct MongoContractDataScope {
    db: Database,
    rbac: SharedRbacService,
}

impl MongoContractDataScope {
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

    /// 包装为合同域可注入的共享 Port。
    ///
    /// # 参数
    /// * `db` - 身份与组织集合所在数据库
    /// * `rbac` - 现有 RBAC 快照服务
    ///
    /// # 返回
    /// 返回合同范围 Port。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// 解析必须调用 DataScopeService；不得把身份实体交给合同域。
    pub fn shared(db: Database, rbac: SharedRbacService) -> Arc<dyn ContractDataScopePort> {
        Arc::new(Self::new(db, rbac))
    }
}

#[async_trait]
impl ContractDataScopePort for MongoContractDataScope {
    fn allows(
        &self,
        scope: &ContractResolvedScope,
        object: &ContractScopeObject,
    ) -> erp_contract::Result<bool> {
        evaluate_object(scope, object)
    }

    async fn resolve(
        &self,
        actor: &AuditActor,
        action: &str,
        executor: &mut dyn Executor,
    ) -> erp_contract::Result<ContractResolvedScope> {
        let access = DataScopeService::new(self.db.clone(), self.rbac.clone())
            .resolve(actor, "contract", action, executor)
            .await
            .map_err(map_identity_error)?;
        Ok(ContractResolvedScope {
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
    ) -> erp_contract::Result<BTreeSet<String>> {
        let state = load_organization_state(&self.db, executor).await?;
        expand_org_ids(&state, org_unit_ids, include_descendants).map_err(map_identity_error)
    }

    async fn org_member_ids(
        &self,
        org_unit_ids: &BTreeSet<String>,
        at: Instant,
        executor: &mut dyn Executor,
    ) -> erp_contract::Result<Vec<String>> {
        let state = load_organization_state(&self.db, executor).await?;
        Ok(member_ids(&state, org_unit_ids, at))
    }

    async fn own_org(
        &self,
        user_id: &str,
        at: Instant,
        executor: &mut dyn Executor,
    ) -> erp_contract::Result<Option<String>> {
        let state = load_organization_state(&self.db, executor).await?;
        Ok(state.own_org(user_id, at).map_err(map_identity_error)?.map(str::to_string))
    }
}

/// 转换全部角色条款；任一不支持维度即失败。
///
/// # 参数
/// * `clauses` - 身份域正向范围
///
/// # 返回
/// 返回合同域条款。
///
/// # 错误
/// 出现结算主体或仓库维度时拒绝。
///
/// # 关键业务约束
/// 不支持的维度必须拒绝，不得静默丢弃。
fn map_clauses(clauses: &[ScopeClause]) -> erp_contract::Result<Vec<ContractResolvedClause>> {
    clauses.iter().map(map_clause).collect()
}

/// 将身份域条款转为合同已解析条款。
///
/// # 参数
/// * `clause` - 身份域正向范围
///
/// # 返回
/// 返回合同适用维度。
///
/// # 错误
/// 结算主体或仓库目标非空时拒绝。
///
/// # 关键业务约束
/// 必须保留公司、主责、协作和组织维度；不得改变语义。
fn map_clause(clause: &ScopeClause) -> erp_contract::Result<ContractResolvedClause> {
    if !clause.settlement_party_ids.is_empty() || !clause.warehouse_ids.is_empty() {
        return Err(erp_contract::Error::ValidationError("合同范围不支持结算主体或仓库维度".into()));
    }
    Ok(ContractResolvedClause {
        company: clause.company,
        self_owned: clause.self_owned,
        collaborative: clause.collaborative,
        org_unit_ids: clause.org_unit_ids.iter().cloned().collect(),
    })
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    #[test]
    fn contract_adapter_rejects_unsupported_scope_dimensions() {
        let warehouse =
            ScopeClause { warehouse_ids: BTreeSet::from(["wh-1".into()]), ..ScopeClause::default() };
        match map_clause(&warehouse) {
            Err(erp_contract::Error::ValidationError(message)) => {
                assert!(message.contains("仓库"));
            },
            other => panic!("expected validation error, got {other:?}"),
        }
        let settlement = ScopeClause {
            settlement_party_ids: BTreeSet::from(["party-1".into()]),
            ..ScopeClause::default()
        };
        assert!(matches!(map_clause(&settlement), Err(erp_contract::Error::ValidationError(_))));
    }

    #[test]
    fn contract_adapter_keeps_owner_collab_and_org_dimensions() {
        let clause = ScopeClause {
            company: false,
            self_owned: true,
            collaborative: true,
            org_unit_ids: BTreeSet::from(["org-b".into(), "org-a".into()]),
            ..ScopeClause::default()
        };
        let mapped = map_clause(&clause).unwrap();
        assert!(mapped.self_owned);
        assert!(mapped.collaborative);
        assert_eq!(mapped.org_unit_ids, vec!["org-a".to_string(), "org-b".to_string()]);
    }

    #[test]
    fn identity_forbidden_stays_forbidden() {
        match map_identity_error(erp_identity::Error::Forbidden("没有该资源动作权限".into())) {
            erp_contract::Error::Forbidden(message) => {
                assert_eq!(message, "没有该资源动作权限");
            },
            other => panic!("expected forbidden, got {other:?}"),
        }
    }
}

/// 将本域已解析事实无损转回公共判定输入，不读取或重解释原始规则。
fn evaluate_object(
    scope: &ContractResolvedScope,
    object: &ContractScopeObject,
) -> erp_contract::Result<bool> {
    if scope.resource != "contract" {
        return Err(erp_contract::Error::ValidationError("范围资源与消费方不一致".into()));
    }
    let consumer = registration(&scope.resource, &scope.action).map_err(map_identity_error)?;
    let resolved = ResolvedScope {
        role_clauses: scope.role_clauses.iter().map(public_clause).collect(),
        user_limit: scope.user_limit.as_ref().map(public_clause),
    };
    Ok(resolved.allows(
        &ScopedObject {
            owned: object.owned,
            collaborating: object.collaborating,
            historical_read_participant: object.historical_read_participant,
            org_unit_id: object.org_unit_id.as_deref(),
            settlement_party_id: None,
            warehouse_id: None,
        },
        consumer.allows_history,
    ))
}

/// 转换已解析条款，保留本人、协作、组织及空集。
fn public_clause(clause: &ContractResolvedClause) -> ScopeClause {
    ScopeClause {
        company: clause.company,
        self_owned: clause.self_owned,
        collaborative: clause.collaborative,
        org_unit_ids: clause.org_unit_ids.iter().cloned().collect(),
        ..ScopeClause::default()
    }
}

#[cfg(test)]
mod equivalence_tests {
    use erp_contract::service::contract::access::contract_scope;
    use serde_json::json;
    use test_support::matches_filter as matches;

    use super::*;

    fn clause(mask: u8) -> ContractResolvedClause {
        ContractResolvedClause {
            company: mask & 1 != 0,
            self_owned: mask & 2 != 0,
            collaborative: mask & 4 != 0,
            org_unit_ids: if mask & 8 != 0 { vec!["org-a".into()] } else { vec![] },
        }
    }

    #[test]
    fn unwired_port_and_mismatched_resource_fail_closed() {
        use erp_contract::ports::FailClosedContractDataScopePort;
        let mut scope = ContractResolvedScope {
            user_id: "actor".into(),
            resource: "contract".into(),
            action: "detail".into(),
            role_clauses: vec![clause(1)],
            user_limit: None,
            policy_version: 1,
            organization_version: 1,
            scope_version: "v1".into(),
            as_of: Instant::from_unix_secs(0),
        };
        let object = ContractScopeObject::default();
        assert!(FailClosedContractDataScopePort.allows(&scope, &object).is_err());
        assert!(evaluate_object(&scope, &object).unwrap());
        scope.resource = "work_item".into();
        assert!(evaluate_object(&scope, &object).is_err());
    }

    #[test]
    fn public_object_decision_matches_compiled_conditions() {
        let ids = (0..16).map(|i| format!("o-{i}")).collect::<Vec<_>>();
        let selected = |bit| {
            ids.iter().enumerate().filter(|(i, _)| i & bit != 0).map(|(_, id)| id.clone()).collect::<Vec<_>>()
        };
        let owned = selected(1);
        let collaborating = selected(2);
        let org_owned = vec![(vec!["org-a".into()], selected(4))];
        for action in ["detail", "create", "update"] {
            for role in 0..16 {
                for second in [0, 2, 8] {
                    for limit in -1..16 {
                        let access = ContractResolvedScope {
                            user_id: "actor".into(),
                            resource: "contract".into(),
                            action: action.into(),
                            role_clauses: vec![clause(role), clause(second)],
                            user_limit: (limit >= 0).then(|| clause(limit as u8)),
                            policy_version: 1,
                            organization_version: 1,
                            scope_version: "v1".into(),
                            as_of: Instant::from_unix_secs(0),
                        };
                        let mut history = Vec::new();
                        if action == "detail" {
                            history = selected(8);
                        }
                        if let Some(limit) = &access.user_limit {
                            let mut upper = access.clone();
                            upper.role_clauses = vec![limit.clone()];
                            upper.user_limit = None;
                            let upper =
                                contract_scope(&upper, "actor", &owned, &collaborating, vec![], &org_owned);
                            history.retain(|id| {
                                upper.authorized_customer_ids.as_ref().is_none_or(|ids| ids.contains(id))
                            });
                        }
                        let compiled =
                            contract_scope(&access, "actor", &owned, &collaborating, history, &org_owned);
                        for (index, id) in ids.iter().enumerate() {
                            let object = ContractScopeObject {
                                owned: index & 1 != 0,
                                collaborating: index & 2 != 0,
                                historical_read_participant: index & 8 != 0,
                                org_unit_id: Some(if index & 4 != 0 { "org-a" } else { "org-b" }.into()),
                            };
                            let document = json!({ "id": id, "customer_id": id,
                            "owner_user_id": if object.owned { "actor" } else { "other" },
                            "business_org_unit_id": object.org_unit_id.as_deref().unwrap() });
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
