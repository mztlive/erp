//! 供给范围授权 adapter：调用身份域公共解析器，转换成供给 Port 事实。

use std::collections::BTreeSet;
use std::sync::Arc;

use async_trait::async_trait;
use erp_core::common::time::Instant;
use erp_identity::SharedRbacService;
use erp_identity::access_control::{ResolvedScope, ScopeClause, ScopedObject};
use erp_identity::entity::organization::OrgTree;
use erp_identity::entity::organization_change::OrganizationState;
use erp_identity::repository::OrganizationRepository;
use erp_identity::service::access_control::consumers::registration;
use erp_identity::service::access_control::resolve::DataScopeService;
use erp_supply::{
    OfferingAccess, OfferingDataScopePort, OfferingResolvedClause, OfferingResolvedScope,
    OfferingScopeObject, SupplierOfferingService,
};
use mongodb::Database;
use persistence_core::Executor;

use crate::supply_governance::SupplierOfferingProcess;

/// 组合层供给范围 adapter，持有身份域解析所需依赖。
#[derive(Clone)]
pub struct MongoOfferingDataScope {
    db: Database,
    rbac: SharedRbacService,
}

impl MongoOfferingDataScope {
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

    /// 包装为供给域可注入的共享 Port。
    ///
    /// # 参数
    /// * `db` - 身份与组织集合所在数据库
    /// * `rbac` - 现有 RBAC 快照服务
    ///
    /// # 返回
    /// 返回供给范围 Port。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// 解析必须调用 DataScopeService；不得把身份实体交给供给域。
    pub fn shared(db: Database, rbac: SharedRbacService) -> Arc<dyn OfferingDataScopePort> {
        Arc::new(Self::new(db, rbac))
    }
}

#[async_trait]
impl OfferingDataScopePort for MongoOfferingDataScope {
    fn allows(
        &self,
        scope: &OfferingResolvedScope,
        object: &OfferingScopeObject,
    ) -> erp_supply::Result<bool> {
        evaluate_object(scope, object)
    }

    async fn resolve(
        &self,
        actor: &application_core::AuditActor,
        action: &str,
        executor: &mut dyn Executor,
    ) -> erp_supply::Result<OfferingResolvedScope> {
        let access = DataScopeService::new(self.db.clone(), self.rbac.clone())
            .resolve(actor, "supplier_offering", action, executor)
            .await
            .map_err(map_identity_error)?;
        map_access(access)
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

/// 在调用方事务内读取组织快照。
///
/// # 参数
/// * `db` - 身份数据库
/// * `executor` - 调用方执行器
///
/// # 返回
/// 返回同一事务中的组织状态。
///
/// # 错误
/// 组织集合读取失败时拒绝。
///
/// # 关键业务约束
/// 不得另开事务或换成 `NoTransaction`。
async fn organization_state(
    db: &Database,
    executor: &mut dyn Executor,
) -> erp_supply::Result<OrganizationState> {
    Ok(OrganizationRepository::new(db).state(executor).await?)
}

/// 展开启用组织及其可选下级。
///
/// # 参数
/// * `state` - 当前组织事实
/// * `org_ids` - 请求中的组织 ID
/// * `include_descendants` - 是否包含有效下级
///
/// # 返回
/// 返回启用节点的组织 ID 集合。
///
/// # 错误
/// 未知组织或组织树非法时拒绝。
///
/// # 关键业务约束
/// 筛选只能收窄授权结果，不得忽略未知组织。
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

/// 读取指定组织在给定时点的有效主属成员。
///
/// # 参数
/// * `state` - 组织事实
/// * `org_ids` - 内部组织集合
/// * `at` - 授权时点
///
/// # 返回
/// 返回排序去重后的人员 ID。
///
/// # 错误
/// 无。
///
/// # 关键业务约束
/// 过期或未生效成员不得进入当前维护人组织筛选。
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

/// 将身份域已解析授权转换为供给 Port 事实。
///
/// # 参数
/// * `access` - 身份域公共解析结果
///
/// # 返回
/// 返回供给适用维度。
///
/// # 错误
/// 出现结算主体或仓库维度时拒绝。
///
/// # 关键业务约束
/// 不支持的维度必须拒绝，不得静默丢弃或与部门 ID 求并。
fn map_access(
    access: erp_identity::service::access_control::resolve::AuthorizedDataScope,
) -> erp_supply::Result<OfferingResolvedScope> {
    Ok(OfferingResolvedScope {
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
fn map_clauses(clauses: &[ScopeClause]) -> erp_supply::Result<Vec<OfferingResolvedClause>> {
    clauses.iter().map(map_clause).collect()
}

/// 将身份域条款转为供给已解析条款。
///
/// # 参数
/// * `clause` - 身份域正向范围
///
/// # 返回
/// 返回供给适用维度。
///
/// # 错误
/// 结算主体或仓库目标非空时拒绝。
///
/// # 关键业务约束
/// 必须保留公司、本人负责、协作和组织维度；不得改变语义。
fn map_clause(clause: &ScopeClause) -> erp_supply::Result<OfferingResolvedClause> {
    if !clause.settlement_party_ids.is_empty() || !clause.warehouse_ids.is_empty() {
        return Err(erp_supply::Error::ValidationError("供给范围不支持结算主体或仓库维度".into()));
    }
    Ok(OfferingResolvedClause {
        company: clause.company,
        self_owned: clause.self_owned,
        collaborative: clause.collaborative,
        org_unit_ids: clause.org_unit_ids.iter().cloned().collect(),
    })
}

/// 将身份域错误映射为供给领域错误。
///
/// # 参数
/// * `error` - 身份域错误
///
/// # 返回
/// 返回同构载荷的供给错误；RBAC 内部失败归入系统错误。
///
/// # 错误
/// 无。
///
/// # 关键业务约束
/// 不得把身份域 Forbidden 改写成校验通过后的空集。
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

/// 将本域已解析事实无损转回公共判定输入，不读取或重解释原始规则。
fn evaluate_object(scope: &OfferingResolvedScope, object: &OfferingScopeObject) -> erp_supply::Result<bool> {
    if scope.resource != "supplier_offering" {
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

/// 转换已解析条款，保留本人、协作、组织及空集。
fn public_clause(clause: &OfferingResolvedClause) -> ScopeClause {
    ScopeClause {
        company: clause.company,
        self_owned: clause.self_owned,
        collaborative: clause.collaborative,
        org_unit_ids: clause.org_unit_ids.iter().cloned().collect(),
        ..ScopeClause::default()
    }
}

/// 构造绑定身份数据库的供给访问器。
///
/// # 参数
/// * `db` - 数据库
/// * `rbac` - 当前 RBAC 快照
///
/// # 返回
/// 返回已注入本 adapter 的供给访问器。
///
/// # 错误
/// 无。
///
/// # 关键业务约束
/// HTTP 与命名 Process 必须经此入口，不得把 RBAC 直接交给供给域。
pub fn offering_access(db: Database, rbac: SharedRbacService) -> OfferingAccess {
    OfferingAccess::new(db.clone(), MongoOfferingDataScope::shared(db, rbac))
}

/// 装配已接线范围 Port 的供给服务。
///
/// # 参数
/// * `db` - 数据库
/// * `rbac` - 当前 RBAC 快照
///
/// # 返回
/// 返回注入生产范围 adapter 的供给服务。
///
/// # 错误
/// 无。
pub fn scoped_offering_service(db: Database, rbac: SharedRbacService) -> SupplierOfferingService {
    SupplierOfferingService::new(db.clone()).with_data_scope(MongoOfferingDataScope::shared(db, rbac))
}

/// 装配已接线范围 Port 的供给治理流程。
///
/// # 参数
/// * `db` - 数据库
/// * `rbac` - 当前 RBAC 快照
///
/// # 返回
/// 返回注入生产范围 adapter 的供给流程。
///
/// # 错误
/// 无。
pub fn scoped_offering_process(db: Database, rbac: SharedRbacService) -> SupplierOfferingProcess {
    SupplierOfferingProcess::new(db.clone()).with_data_scope(MongoOfferingDataScope::shared(db, rbac))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offering_adapter_rejects_unsupported_scope_dimensions() {
        let warehouse =
            ScopeClause { warehouse_ids: BTreeSet::from(["wh-1".into()]), ..ScopeClause::default() };
        assert!(matches!(map_clause(&warehouse), Err(erp_supply::Error::ValidationError(_))));
        let settlement = ScopeClause {
            settlement_party_ids: BTreeSet::from(["party-1".into()]),
            ..ScopeClause::default()
        };
        assert!(matches!(map_clause(&settlement), Err(erp_supply::Error::ValidationError(_))));
    }

    #[test]
    fn offering_adapter_keeps_owner_and_org_dimensions() {
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
            erp_supply::Error::Forbidden(message) => assert_eq!(message, "没有该资源动作权限"),
            other => panic!("expected forbidden, got {other:?}"),
        }
    }

    #[test]
    fn a34_public_allows_matches_offering_read_scope_document() {
        use erp_core::common::time::Instant;
        use erp_supply::offering_scope;
        use serde_json::json;
        use test_support::matches_filter as matches;

        let clause = OfferingResolvedClause {
            company: false,
            self_owned: true,
            collaborative: false,
            org_unit_ids: vec!["org-a".into()],
        };
        let scope = OfferingResolvedScope {
            user_id: "actor".into(),
            resource: "supplier_offering".into(),
            action: "list".into(),
            role_clauses: vec![clause.clone()],
            user_limit: None,
            policy_version: 1,
            organization_version: 1,
            scope_version: "v1".into(),
            as_of: Instant::from_unix_secs(0),
        };
        let read = offering_scope(&scope, "actor");
        for (owner, org, expected) in
            [("actor", "org-b", true), ("other", "org-a", true), ("other", "org-b", false)]
        {
            let object = OfferingScopeObject { owned: owner == "actor", org_unit_id: Some(org.into()) };
            let document = json!({
                "id": "offering-1",
                "maintainer_user_id": owner,
                "business_org_unit_id": org,
            });
            assert_eq!(evaluate_object(&scope, &object).unwrap(), expected);
            assert_eq!(matches(&read.document(), &document), expected);
            assert_eq!(read.allows_object(owner, org), expected);
        }
    }
}
