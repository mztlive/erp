//! 资金往来范围授权 adapter：调用身份域公共解析器，转换成资金 Port 事实。

use std::collections::BTreeSet;
use std::sync::Arc;

use application_core::AuditActor;
use async_trait::async_trait;
use erp_core::common::time::Instant;
use erp_finance::ports::funds_scope::{
    FundsDataScopePort, FundsResolvedClause, FundsResolvedScope, FundsScopeObject,
};
use erp_identity::SharedRbacService;
use erp_identity::access_control::{ScopeClause, ScopedObject};
use erp_identity::service::access_control::resolve::DataScopeService;
use mongodb::Database;
use persistence_core::Executor;

use super::identity_error::map_identity_error;
use super::scope_support::{self, evaluate_registered, reject_unsupported_dimensions, scope_clause};

map_identity_error!(erp_finance);

/// 组合层资金范围 adapter，持有身份域解析所需依赖。
#[derive(Clone)]
pub struct MongoFundsDataScope {
    db: Database,
    rbac: SharedRbacService,
}

impl MongoFundsDataScope {
    /// 绑定身份数据库及现有 RBAC 实例。
    pub fn new(db: Database, rbac: SharedRbacService) -> Self {
        Self { db, rbac }
    }

    /// 包装为资金域可注入的共享 Port。
    pub fn shared(db: Database, rbac: SharedRbacService) -> Arc<dyn FundsDataScopePort> {
        Arc::new(Self::new(db, rbac))
    }
}

#[async_trait]
impl FundsDataScopePort for MongoFundsDataScope {
    fn allows(&self, scope: &FundsResolvedScope, object: &FundsScopeObject) -> erp_finance::Result<bool> {
        evaluate_object(scope, object)
    }

    async fn resolve(
        &self,
        actor: &AuditActor,
        resource: &str,
        action: &str,
        executor: &mut dyn Executor,
    ) -> erp_finance::Result<FundsResolvedScope> {
        let access = DataScopeService::new(self.db.clone(), self.rbac.clone())
            .resolve(actor, resource, action, executor)
            .await
            .map_err(map_identity_error)?;
        Ok(FundsResolvedScope {
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
    ) -> erp_finance::Result<BTreeSet<String>> {
        scope_support::expand_org_units(
            &self.db,
            org_unit_ids,
            include_descendants,
            executor,
            erp_finance::Error::RepositoryError,
            map_identity_error,
        )
        .await
    }

    async fn org_member_ids(
        &self,
        org_unit_ids: &BTreeSet<String>,
        at: Instant,
        executor: &mut dyn Executor,
    ) -> erp_finance::Result<Vec<String>> {
        scope_support::org_member_ids(
            &self.db,
            org_unit_ids,
            at,
            executor,
            erp_finance::Error::RepositoryError,
        )
        .await
    }
}

/// 转换全部角色条款；任一不支持维度即失败。
fn map_clauses(clauses: &[ScopeClause]) -> erp_finance::Result<Vec<FundsResolvedClause>> {
    clauses.iter().map(map_clause).collect()
}

/// 将身份域条款转为资金已解析条款。
fn map_clause(clause: &ScopeClause) -> erp_finance::Result<FundsResolvedClause> {
    reject_unsupported_dimensions(clause, "资金范围不支持结算主体或仓库维度")
        .map_err(erp_finance::Error::ValidationError)?;
    Ok(FundsResolvedClause {
        company: clause.company,
        self_owned: clause.self_owned,
        collaborative: clause.collaborative,
        org_unit_ids: clause.org_unit_ids.iter().cloned().collect(),
    })
}

/// 将本域已解析事实无损转回公共判定输入，不读取或重解释原始规则。
///
/// # 参数
/// * `scope` - 当前动作已解析事实
/// * `object` - 本域提供的关联责任事实
///
/// # 返回
/// 返回角色范围和个人上限共同允许的判定。
///
/// # 错误
/// 资源不符或未接线动作拒绝。
fn evaluate_object(scope: &FundsResolvedScope, object: &FundsScopeObject) -> erp_finance::Result<bool> {
    ensure_funds_resource(&scope.resource)?;
    evaluate_registered(
        &scope.resource,
        &scope.action,
        scope.role_clauses.iter().map(public_clause),
        scope.user_limit.as_ref().map(public_clause),
        ScopedObject {
            owned: object.owned,
            collaborating: object.collaborating,
            historical_read_participant: false,
            org_unit_id: object.org_unit_id.as_deref(),
            settlement_party_id: None,
            warehouse_id: None,
        },
        map_identity_error,
    )
}

/// 校验资金资源名；采购关联仍按资金资源动作证明。
///
/// # 参数
/// * `resource` - 请求资源
///
/// # 返回
/// 资金往来七资源时通过。
///
/// # 错误
/// 其他资源返回校验错误。
fn ensure_funds_resource(resource: &str) -> erp_finance::Result<()> {
    if matches!(
        resource,
        "receivable_account"
            | "customer_receipt"
            | "invoice"
            | "sales_invoice_request"
            | "payable_account"
            | "supplier_payment"
            | "purchase_invoice_allocation"
    ) {
        return Ok(());
    }
    Err(erp_finance::Error::ValidationError("范围资源与资金消费方不一致".into()))
}

/// 转换已解析条款，保留本人、协作、组织及空集。
fn public_clause(clause: &FundsResolvedClause) -> ScopeClause {
    scope_clause(clause.company, clause.self_owned, clause.collaborative, &clause.org_unit_ids)
}

/// 构造绑定同一 RBAC 的资金访问器；HTTP 与命名 Process 必须经此入口。
pub fn funds_access_with_rbac(
    db: Database,
    rbac: SharedRbacService,
) -> erp_read_models::finance::funds_scope::FundsAccess {
    let scope = MongoFundsDataScope::shared(db.clone(), rbac.clone());
    erp_read_models::finance::funds_scope::FundsAccess::new(db, rbac, scope)
}

#[cfg(test)]
mod equivalence_tests {
    use erp_finance::ports::funds_scope::FundsResolvedScope;
    use erp_read_models::finance::funds_scope::{FundsAccess, FundsLinkedFacts};
    use erp_sales::repository::sales_order::scope::{SalesReadScope, SalesScopeClause};
    use serde_json::json;
    use test_support::matches_filter as matches;

    use super::*;

    fn clause(mask: u8) -> FundsResolvedClause {
        FundsResolvedClause {
            company: mask & 1 != 0,
            self_owned: mask & 2 != 0,
            collaborative: mask & 4 != 0,
            org_unit_ids: if mask & 8 != 0 { vec!["org-a".into()] } else { vec![] },
        }
    }

    fn scope_clause(mask: u8, actor: &str) -> SalesScopeClause {
        SalesScopeClause {
            company: mask & 1 != 0,
            owner_user_id: (mask & 2 != 0).then(|| actor.to_string()),
            business_org_unit_ids: if mask & 8 != 0 { vec!["org-a".into()] } else { vec![] },
            collaborative_customer_ids: Vec::new(),
        }
    }

    /// S3-08 A34：资金公共单对象判定与销售条件编译集合等价。
    ///
    /// # 参数
    /// 无；16 对象×多角色×个人上限全组合内存断言。
    ///
    /// # 返回
    /// 无；等价时通过。
    ///
    /// # 错误
    /// 不等价时失败。
    ///
    /// # 关键业务约束
    /// 以公共判定为基准；非真实库执行等价，真实库对拍转上线准入跟踪。
    #[test]
    fn public_object_decision_matches_compiled_conditions() {
        use erp_finance::ports::funds_scope::FundsScopeObject;
        let ids = (0..16).map(|i| format!("o-{i}")).collect::<Vec<_>>();
        for action in ["list", "detail"] {
            for role in 0..16 {
                for second in [0, 2, 8] {
                    for limit in -1_i32..16 {
                        let access = FundsResolvedScope {
                            user_id: "actor".into(),
                            resource: "customer_receipt".into(),
                            action: action.into(),
                            role_clauses: vec![clause(role), clause(second)],
                            user_limit: (limit >= 0).then(|| clause(limit as u8)),
                            policy_version: 1,
                            organization_version: 1,
                            scope_version: "v1".into(),
                            as_of: Instant::from_unix_secs(0),
                        };
                        let compiled = SalesReadScope {
                            roles: vec![scope_clause(role, "actor"), scope_clause(second, "actor")],
                            user_limit: (limit >= 0).then(|| scope_clause(limit as u8, "actor")),
                            historical_order_ids: vec![],
                            required_scopes: vec![],
                        };
                        for (index, id) in ids.iter().enumerate() {
                            let facts = FundsLinkedFacts {
                                owner_user_id: (index & 1 != 0).then(|| "actor".into()),
                                business_org_unit_id: Some(
                                    (if index & 4 != 0 { "org-a" } else { "org-b" }).into(),
                                ),
                                linked_document_id: id.clone(),
                                ..FundsLinkedFacts::default()
                            };
                            let object = FundsScopeObject {
                                owned: index & 1 != 0,
                                collaborating: false,
                                org_unit_id: facts.business_org_unit_id.clone(),
                            };
                            let owner = if index & 1 != 0 { "actor" } else { "other" };
                            let org = facts.business_org_unit_id.as_deref().unwrap();
                            let document = json!({ "id": id,
                            "sales_owner_user_id": owner,
                            "business_org_unit_id": org });
                            assert_eq!(
                                FundsAccess::allows(&access, &facts).unwrap(),
                                matches(&compiled.document(), &document),
                                "action={action}, role={role}, second={second}, limit={limit}, object={index}"
                            );
                            assert_eq!(
                                evaluate_object(&access, &object).unwrap(),
                                FundsAccess::allows(&access, &facts).unwrap(),
                                "adapter 与唯一对象映射一致 action={action}, object={index}"
                            );
                        }
                    }
                }
            }
        }
    }

    /// S3-08 A35：未装配 Port 与资源不符失败关闭，不补 Company。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 无；关闭时通过。
    ///
    /// # 错误
    /// 回退或放行时失败。
    ///
    /// # 关键业务约束
    /// 禁止捕获错误后回退旧读取器或 Company。
    #[test]
    fn unwired_port_and_mismatched_resource_fail_closed() {
        use erp_finance::ports::funds_scope::{FailClosedFundsDataScopePort, FundsScopeObject};
        let mut scope = FundsResolvedScope {
            user_id: "actor".into(),
            resource: "customer_receipt".into(),
            action: "list".into(),
            role_clauses: vec![clause(1)],
            user_limit: None,
            policy_version: 1,
            organization_version: 1,
            scope_version: "v1".into(),
            as_of: Instant::from_unix_secs(0),
        };
        let object = FundsScopeObject::default();
        assert!(FailClosedFundsDataScopePort.allows(&scope, &object).is_err());
        assert!(ensure_funds_resource(&scope.resource).is_ok());
        scope.resource = "work_item".into();
        assert!(ensure_funds_resource(&scope.resource).is_err());
        assert!(evaluate_object(&scope, &object).is_err());
    }
}

#[cfg(test)]
mod adapter_tests {
    use std::collections::BTreeSet;

    use super::*;

    #[test]
    fn funds_adapter_rejects_unsupported_scope_dimensions() {
        let warehouse =
            ScopeClause { warehouse_ids: BTreeSet::from(["wh-1".into()]), ..ScopeClause::default() };
        match map_clause(&warehouse) {
            Err(erp_finance::Error::ValidationError(message)) => {
                assert!(message.contains("仓库"));
            },
            other => panic!("expected validation error, got {other:?}"),
        }
        let settlement = ScopeClause {
            settlement_party_ids: BTreeSet::from(["party-1".into()]),
            ..ScopeClause::default()
        };
        assert!(matches!(map_clause(&settlement), Err(erp_finance::Error::ValidationError(_))));
    }

    #[test]
    fn funds_adapter_keeps_owner_collab_and_org_dimensions() {
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
    fn mismatched_resource_fails_closed() {
        let scope = FundsResolvedScope {
            user_id: "actor".into(),
            resource: "work_item".into(),
            action: "list".into(),
            role_clauses: vec![FundsResolvedClause { company: true, ..FundsResolvedClause::default() }],
            user_limit: None,
            policy_version: 1,
            organization_version: 1,
            scope_version: "v1".into(),
            as_of: Instant::from_unix_secs(0),
        };
        assert!(evaluate_object(&scope, &FundsScopeObject::default()).is_err());
    }
}
