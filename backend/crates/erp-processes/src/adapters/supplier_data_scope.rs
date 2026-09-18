//! 供应商范围授权 adapter：调用身份域公共解析器，转换成供应商 Port 事实。

use std::collections::BTreeSet;
use std::sync::Arc;

use application_core::AuditActor;
use async_trait::async_trait;
use erp_core::common::time::Instant;
use erp_identity::SharedRbacService;
use erp_identity::access_control::{ResolvedScope, ScopeClause, ScopedObject};
use erp_identity::service::access_control::consumers::registration;
use erp_identity::service::access_control::resolve::DataScopeService;
use erp_supplier::ports::SupplierScopeObject;
use erp_supplier::{
    SupplierAccess, SupplierDataScopePort, SupplierResolvedClause, SupplierResolvedScope, SupplierService,
};
use mongodb::Database;
use persistence_core::Executor;

use super::scope_support::{expand_org_ids, load_organization_state, member_ids};

/// 组合层供应商范围 adapter，持有身份域解析所需依赖。
#[derive(Clone)]
pub struct MongoSupplierDataScope {
    db: Database,
    rbac: SharedRbacService,
}

impl MongoSupplierDataScope {
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
    pub fn new(db: Database, rbac: SharedRbacService) -> Self {
        Self { db, rbac }
    }

    /// 包装为供应商域可注入的共享 Port。
    ///
    /// # 参数
    /// * `db` - 身份与组织集合所在数据库
    /// * `rbac` - 现有 RBAC 快照服务
    ///
    /// # 返回
    /// 返回供应商范围 Port。
    ///
    /// # 错误
    /// 无。
    pub fn shared(db: Database, rbac: SharedRbacService) -> Arc<dyn SupplierDataScopePort> {
        Arc::new(Self::new(db, rbac))
    }
}

#[async_trait]
impl SupplierDataScopePort for MongoSupplierDataScope {
    fn allows(
        &self,
        scope: &SupplierResolvedScope,
        object: &SupplierScopeObject,
    ) -> erp_supplier::Result<bool> {
        evaluate_object(scope, object)
    }

    async fn resolve(
        &self,
        actor: &AuditActor,
        action: &str,
        executor: &mut dyn Executor,
    ) -> erp_supplier::Result<SupplierResolvedScope> {
        let access = DataScopeService::new(self.db.clone(), self.rbac.clone())
            .resolve(actor, "supplier", action, executor)
            .await
            .map_err(map_identity_error)?;
        Ok(SupplierResolvedScope {
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
    ) -> erp_supplier::Result<BTreeSet<String>> {
        let state = load_organization_state(&self.db, executor).await?;
        expand_org_ids(&state, org_unit_ids, include_descendants).map_err(map_identity_error)
    }

    async fn org_member_ids(
        &self,
        org_unit_ids: &BTreeSet<String>,
        at: Instant,
        executor: &mut dyn Executor,
    ) -> erp_supplier::Result<Vec<String>> {
        let state = load_organization_state(&self.db, executor).await?;
        Ok(member_ids(&state, org_unit_ids, at))
    }

    async fn own_org(
        &self,
        user_id: &str,
        at: Instant,
        executor: &mut dyn Executor,
    ) -> erp_supplier::Result<Option<String>> {
        let state = load_organization_state(&self.db, executor).await?;
        Ok(state.own_org(user_id, at).map_err(map_identity_error)?.map(str::to_string))
    }
}

fn map_clauses(clauses: &[ScopeClause]) -> erp_supplier::Result<Vec<SupplierResolvedClause>> {
    clauses.iter().map(map_clause).collect()
}

fn map_clause(clause: &ScopeClause) -> erp_supplier::Result<SupplierResolvedClause> {
    if !clause.settlement_party_ids.is_empty() || !clause.warehouse_ids.is_empty() {
        return Err(erp_supplier::Error::ValidationError("供应商范围不支持结算主体或仓库维度".into()));
    }
    Ok(SupplierResolvedClause {
        company: clause.company,
        self_owned: clause.self_owned,
        collaborative: clause.collaborative,
        org_unit_ids: clause.org_unit_ids.iter().cloned().collect(),
    })
}

fn map_identity_error(error: erp_identity::Error) -> erp_supplier::Error {
    match error {
        erp_identity::Error::Internal(payload) => erp_supplier::Error::Internal(payload),
        erp_identity::Error::NotFound(payload) => erp_supplier::Error::NotFound(payload),
        erp_identity::Error::ValidationError(payload) => erp_supplier::Error::ValidationError(payload),
        erp_identity::Error::BusinessLogicError(payload) => erp_supplier::Error::BusinessLogicError(payload),
        erp_identity::Error::ConflictError(payload) => erp_supplier::Error::ConflictError(payload),
        erp_identity::Error::ReceiptDuplicate(payload) => erp_supplier::Error::ReceiptDuplicate(payload),
        erp_identity::Error::TransientTransaction(payload) => {
            erp_supplier::Error::TransientTransaction(payload)
        },
        erp_identity::Error::Forbidden(payload) => erp_supplier::Error::Forbidden(payload),
        erp_identity::Error::Unauthenticated(payload) => erp_supplier::Error::Unauthenticated(payload),
        erp_identity::Error::Logic(payload) => erp_supplier::Error::Logic(payload),
        erp_identity::Error::Rbac(payload) => erp_supplier::Error::Internal(payload),
        erp_identity::Error::OutcomeUnknown(payload) => erp_supplier::Error::OutcomeUnknown(payload),
        erp_identity::Error::RepositoryError(payload) => erp_supplier::Error::RepositoryError(payload),
    }
}

fn evaluate_object(
    scope: &SupplierResolvedScope,
    object: &SupplierScopeObject,
) -> erp_supplier::Result<bool> {
    if scope.resource != "supplier" {
        return Err(erp_supplier::Error::ValidationError("范围资源与消费方不一致".into()));
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
            historical_read_participant: object.historical_read_participant,
            org_unit_id: object.org_unit_id.as_deref(),
            settlement_party_id: None,
            warehouse_id: None,
        },
        consumer.allows_history,
    ))
}

fn public_clause(clause: &SupplierResolvedClause) -> ScopeClause {
    ScopeClause {
        company: clause.company,
        self_owned: clause.self_owned,
        collaborative: clause.collaborative,
        org_unit_ids: clause.org_unit_ids.iter().cloned().collect(),
        ..ScopeClause::default()
    }
}

/// 构造绑定身份数据库的供应商访问器。
///
/// # 参数
/// * `db` - 供应商与身份集合所在数据库
/// * `rbac` - 现有 RBAC 快照服务
///
/// # 返回
/// 返回未缓存授权的供应商访问器。
///
/// # 错误
/// 无。
pub fn supplier_access(db: Database, rbac: SharedRbacService) -> SupplierAccess {
    SupplierAccess::new(db.clone(), MongoSupplierDataScope::shared(db, rbac))
}

/// 构造已接入身份域公共解析器的供应商服务。
///
/// # 参数
/// * `db` - 供应商与身份集合所在数据库
/// * `rbac` - 现有 RBAC 快照服务
///
/// # 返回
/// 返回可解析供应商范围的服务。
///
/// # 错误
/// 无。
pub fn scoped_supplier_service(db: Database, rbac: SharedRbacService) -> SupplierService {
    SupplierService::new(db.clone(), super::MongoSupplierPartyFacts::shared(db.clone())).with_scope(
        MongoSupplierDataScope::shared(db.clone(), rbac),
        super::supplier::MongoSupplierAccountFacts::shared(db),
    )
}

/// 构造可签发敏感字段短时揭示令牌且已接入范围的供应商服务。
///
/// # 参数
/// * `db` - 供应商与身份集合所在数据库
/// * `rbac` - 现有 RBAC 快照服务
/// * `sensitive_data` - 敏感字段编解码
///
/// # 返回
/// 返回可解析供应商范围并签发揭示令牌的服务。
///
/// # 错误
/// 无。
pub fn scoped_supplier_service_with_sensitive(
    db: Database,
    rbac: SharedRbacService,
    sensitive_data: Arc<erp_party::SensitiveDataCodec>,
) -> SupplierService {
    SupplierService::with_sensitive_data(
        db.clone(),
        super::MongoSupplierPartyFacts::shared(db.clone()),
        super::MongoSupplierSensitiveTokens::shared(sensitive_data),
    )
    .with_scope(
        MongoSupplierDataScope::shared(db.clone(), rbac),
        super::supplier::MongoSupplierAccountFacts::shared(db),
    )
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    #[test]
    fn supplier_adapter_rejects_unsupported_scope_dimensions() {
        let warehouse =
            ScopeClause { warehouse_ids: BTreeSet::from(["wh-1".into()]), ..ScopeClause::default() };
        match map_clause(&warehouse) {
            Err(erp_supplier::Error::ValidationError(message)) => assert!(message.contains("仓库")),
            other => panic!("expected validation error, got {other:?}"),
        }
    }

    #[test]
    fn identity_forbidden_stays_forbidden() {
        match map_identity_error(erp_identity::Error::Forbidden("没有该资源动作权限".into())) {
            erp_supplier::Error::Forbidden(message) => assert_eq!(message, "没有该资源动作权限"),
            other => panic!("expected forbidden, got {other:?}"),
        }
    }
}

#[cfg(test)]
mod equivalence_tests {
    use erp_supplier::{SupplierReadScope, SupplierScopeClause, supplier_scope};
    use serde_json::json;
    use test_support::matches_filter as matches;

    use super::*;

    fn clause(mask: u8) -> SupplierResolvedClause {
        SupplierResolvedClause {
            company: mask & 1 != 0,
            self_owned: mask & 2 != 0,
            collaborative: mask & 4 != 0,
            org_unit_ids: if mask & 8 != 0 { vec!["org-a".into()] } else { vec![] },
        }
    }

    fn scope_clause(mask: u8, actor: &str) -> SupplierScopeClause {
        SupplierScopeClause {
            company: mask & 1 != 0,
            owner_user_id: (mask & 2 != 0).then(|| actor.to_string()),
            business_org_unit_ids: if mask & 8 != 0 { vec!["org-a".into()] } else { vec![] },
        }
    }

    #[test]
    fn unwired_port_and_mismatched_resource_fail_closed() {
        use erp_supplier::FailClosedSupplierDataScopePort;
        let mut scope = SupplierResolvedScope {
            user_id: "actor".into(),
            resource: "supplier".into(),
            action: "detail".into(),
            role_clauses: vec![clause(1)],
            user_limit: None,
            policy_version: 1,
            organization_version: 1,
            scope_version: "v1".into(),
            as_of: Instant::from_unix_secs(0),
        };
        let object = SupplierScopeObject::default();
        assert!(FailClosedSupplierDataScopePort.allows(&scope, &object).is_err());
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
                        let access = SupplierResolvedScope {
                            user_id: "actor".into(),
                            resource: "supplier".into(),
                            action: action.into(),
                            role_clauses: vec![clause(role), clause(second)],
                            user_limit: (limit >= 0).then(|| clause(limit as u8)),
                            policy_version: 1,
                            organization_version: 1,
                            scope_version: "v1".into(),
                            as_of: Instant::from_unix_secs(0),
                        };
                        let compiled = SupplierReadScope {
                            roles: vec![scope_clause(role, "actor"), scope_clause(second, "actor")],
                            user_limit: (limit >= 0).then(|| scope_clause(limit as u8, "actor")),
                        };
                        assert_eq!(
                            supplier_scope(&access, "actor"),
                            compiled,
                            "action={action}, role={role}, second={second}, limit={limit}"
                        );
                        for (index, id) in ids.iter().enumerate() {
                            let object = SupplierScopeObject {
                                owned: index & 1 != 0,
                                historical_read_participant: false,
                                org_unit_id: Some(if index & 4 != 0 { "org-a" } else { "org-b" }.into()),
                            };
                            let owner = if object.owned { "actor" } else { "other" };
                            let org = object.org_unit_id.as_deref().unwrap();
                            let document = json!({
                                "id": id,
                                "maintainer_user_id": owner,
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
