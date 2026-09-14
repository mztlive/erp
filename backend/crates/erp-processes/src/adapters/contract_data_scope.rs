//! 合同范围授权 adapter：调用身份域公共解析器，转换成合同 Port 事实。

use std::collections::BTreeSet;
use std::sync::Arc;

use application_core::AuditActor;
use async_trait::async_trait;
use erp_contract::{ContractDataScopePort, ContractResolvedClause, ContractResolvedScope};
use erp_core::common::time::Instant;
use erp_identity::access_control::ScopeClause;
use erp_identity::entity::organization::OrgTree;
use erp_identity::entity::organization_change::OrganizationState;
use erp_identity::repository::OrganizationRepository;
use erp_identity::service::access_control::resolve::DataScopeService;
use erp_identity::SharedRbacService;
use mongodb::Database;
use persistence_core::Executor;

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
        let state = organization_state(&self.db, executor).await?;
        expand_org_units(&state, org_unit_ids, include_descendants)
    }

    async fn org_member_ids(
        &self,
        org_unit_ids: &BTreeSet<String>,
        at: Instant,
        executor: &mut dyn Executor,
    ) -> erp_contract::Result<Vec<String>> {
        let state = organization_state(&self.db, executor).await?;
        Ok(member_ids(&state, org_unit_ids, at))
    }

    async fn own_org(
        &self,
        user_id: &str,
        at: Instant,
        executor: &mut dyn Executor,
    ) -> erp_contract::Result<Option<String>> {
        let state = organization_state(&self.db, executor).await?;
        Ok(state
            .own_org(user_id, at)
            .map_err(map_identity_error)?
            .map(str::to_string))
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
) -> erp_contract::Result<OrganizationState> {
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
) -> erp_contract::Result<BTreeSet<String>> {
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
/// 过期或未生效成员不得进入当前主责组织筛选。
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
        return Err(erp_contract::Error::ValidationError(
            "合同范围不支持结算主体或仓库维度".into(),
        ));
    }
    Ok(ContractResolvedClause {
        company: clause.company,
        self_owned: clause.self_owned,
        collaborative: clause.collaborative,
        org_unit_ids: clause.org_unit_ids.iter().cloned().collect(),
    })
}

/// 将身份域错误映射为合同领域错误。
///
/// # 参数
/// * `error` - 身份域错误
///
/// # 返回
/// 返回同构载荷的合同错误；RBAC 内部失败归入系统错误。
///
/// # 错误
/// 无。
///
/// # 关键业务约束
/// 不得把身份域 Forbidden 改写成校验通过后的空集。
fn map_identity_error(error: erp_identity::Error) -> erp_contract::Error {
    match error {
        erp_identity::Error::Internal(payload) => erp_contract::Error::Internal(payload),
        erp_identity::Error::NotFound(payload) => erp_contract::Error::NotFound(payload),
        erp_identity::Error::ValidationError(payload) => erp_contract::Error::ValidationError(payload),
        erp_identity::Error::BusinessLogicError(payload) => erp_contract::Error::BusinessLogicError(payload),
        erp_identity::Error::ConflictError(payload) => erp_contract::Error::ConflictError(payload),
        erp_identity::Error::ReceiptDuplicate(payload) => erp_contract::Error::ReceiptDuplicate(payload),
        erp_identity::Error::TransientTransaction(payload) => {
            erp_contract::Error::TransientTransaction(payload)
        }
        erp_identity::Error::Forbidden(payload) => erp_contract::Error::Forbidden(payload),
        erp_identity::Error::Unauthenticated(payload) => erp_contract::Error::Unauthenticated(payload),
        erp_identity::Error::Logic(payload) => erp_contract::Error::Logic(payload),
        erp_identity::Error::Rbac(payload) => erp_contract::Error::Internal(payload),
        erp_identity::Error::OutcomeUnknown(payload) => erp_contract::Error::OutcomeUnknown(payload),
        erp_identity::Error::RepositoryError(payload) => erp_contract::Error::RepositoryError(payload),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn contract_adapter_rejects_unsupported_scope_dimensions() {
        let warehouse = ScopeClause {
            warehouse_ids: BTreeSet::from(["wh-1".into()]),
            ..ScopeClause::default()
        };
        match map_clause(&warehouse) {
            Err(erp_contract::Error::ValidationError(message)) => {
                assert!(message.contains("仓库"));
            }
            other => panic!("expected validation error, got {other:?}"),
        }
        let settlement = ScopeClause {
            settlement_party_ids: BTreeSet::from(["party-1".into()]),
            ..ScopeClause::default()
        };
        assert!(matches!(
            map_clause(&settlement),
            Err(erp_contract::Error::ValidationError(_))
        ));
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
        assert_eq!(
            mapped.org_unit_ids,
            vec!["org-a".to_string(), "org-b".to_string()]
        );
    }

    #[test]
    fn identity_forbidden_stays_forbidden() {
        match map_identity_error(erp_identity::Error::Forbidden("没有该资源动作权限".into())) {
            erp_contract::Error::Forbidden(message) => {
                assert_eq!(message, "没有该资源动作权限");
            }
            other => panic!("expected forbidden, got {other:?}"),
        }
    }
}
