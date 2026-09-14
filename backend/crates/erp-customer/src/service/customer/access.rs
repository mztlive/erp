//! 客户对象读取与写入范围；主责、协作与负责人组织分别解释。

use std::collections::BTreeSet;
use std::hash::{Hash, Hasher};

use application_core::AuditActor;
use erp_core::common::time::{BusinessDate, Instant};
use erp_identity::access_control::ScopeClause;
use erp_identity::entity::organization::OrgTree;
use erp_identity::entity::organization_change::OrganizationState;
use erp_identity::service::access_control::resolve::{AuthorizedDataScope, DataScopeService};
use erp_identity::SharedRbacService;
use mongodb::Database;
use persistence_core::{Executor, Transactional};

use crate::entity::customer::AssignmentRole;
use crate::error::{Error, Result};
use crate::repository::scope::{CustomerReadScope, CustomerScopeClause};
use crate::repository::CustomerExt;

/// 客户对象访问范围；列表、详情、导出和写命令复用同一解析。
#[derive(Clone)]
pub struct CustomerAccess {
    db: Database,
    rbac: SharedRbacService,
}

impl CustomerAccess {
    /// 绑定身份与客户归属事实来源。
    ///
    /// # 参数
    /// * `db` - 身份与客户集合所在数据库
    /// * `rbac` - 现有 RBAC 快照服务
    ///
    /// # 返回
    /// 返回无授权缓存的访问服务，构造不执行 I/O。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// 不得在构造时补公司范围或读取登录人默认组织。
    pub fn new(db: Database, rbac: SharedRbacService) -> Self {
        Self { db, rbac }
    }

    /// 在调用方事务内证明资源动作并映射客户责任条件。
    ///
    /// # 参数
    /// * `actor` - 已认证操作人
    /// * `action` - 已注册的客户动作
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 返回身份上下文和客户授权条件。
    ///
    /// # 错误
    /// 无动作权限返回 Forbidden；查询超限或组织关系非法时拒绝。
    ///
    /// # 关键业务约束
    /// 主责与协作分别解释；组织范围按当前主负责人所属组织展开。
    pub async fn resolve(
        &self,
        actor: &AuditActor,
        action: &str,
        executor: &mut dyn Executor,
    ) -> Result<(AuthorizedDataScope, CustomerReadScope)> {
        let mut access = DataScopeService::new(self.db.clone(), self.rbac.clone())
            .resolve(actor, "customer", action, executor)
            .await?;
        let as_of = business_date(access.as_of)?;
        let assignments = self
            .db
            .customer_assignments()
            .find_active_assignments_for_user(actor.id(), as_of, executor)
            .await?;
        let mut owned = Vec::new();
        let mut collaborating = Vec::new();
        for assignment in assignments {
            match assignment.assignment_role {
                AssignmentRole::Owner => owned.push(assignment.customer_id.to_string()),
                AssignmentRole::Collaborator => collaborating.push(assignment.customer_id.to_string()),
            }
        }
        owned.sort();
        owned.dedup();
        collaborating.sort();
        collaborating.dedup();
        let history = if allows_history(action) {
            self.historical_customers(actor.id(), executor).await?
        } else {
            Vec::new()
        };
        ensure_limit(owned.len(), "主责范围超过查询上限")?;
        ensure_limit(collaborating.len(), "协作范围超过查询上限")?;
        let org_owned = self.org_owned_customers(&access, as_of, executor).await?;
        let mut fingerprint = std::collections::hash_map::DefaultHasher::new();
        owned.hash(&mut fingerprint);
        collaborating.hash(&mut fingerprint);
        history.hash(&mut fingerprint);
        org_owned.hash(&mut fingerprint);
        access.scope_version = format!("{}:{:x}", access.scope_version, fingerprint.finish());
        let scope = customer_scope(&access, actor.id(), &owned, &collaborating, history, &org_owned);
        Ok((access, scope))
    }

    /// 在独立事务中重验对象资格。
    ///
    /// # 参数
    /// * `actor` - 已认证操作人
    /// * `action` - 已注册的客户动作
    /// * `customer_id` - 目标客户
    ///
    /// # 返回
    /// 对象在范围内时返回授权上下文。
    ///
    /// # 错误
    /// 读取动作对不可见对象返回 NotFound；写动作返回 Forbidden。
    ///
    /// # 关键业务约束
    /// 列表已授权不能作为详情或命令的长期凭证；历史参与不授予修改。
    pub async fn require(
        &self,
        actor: &AuditActor,
        action: &str,
        customer_id: &str,
    ) -> Result<AuthorizedDataScope> {
        let this = self.clone();
        let actor = actor.clone();
        let action = action.to_string();
        let customer_id = customer_id.to_string();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move { this.require_with(actor, &action, &customer_id, executor).await })
            })
            .await
    }

    /// 在调用方事务内重验对象资格。
    ///
    /// # 参数
    /// * `actor` - 已认证操作人
    /// * `action` - 已注册的客户动作
    /// * `customer_id` - 目标客户
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 对象在范围内时返回授权上下文。
    ///
    /// # 错误
    /// 账号、动作或对象资格失效时拒绝。
    ///
    /// # 关键业务约束
    /// 写命令必须在原领域事务内调用，不得只依赖请求入口的事前检查。
    pub async fn require_with(
        &self,
        actor: AuditActor,
        action: &str,
        customer_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<AuthorizedDataScope> {
        let (access, scope) = self.resolve(&actor, action, executor).await?;
        let found = self
            .db
            .customer_accounts()
            .find_authorized(customer_id, &scope, executor)
            .await?;
        if found.is_none() {
            return Err(deny_object(action));
        }
        Ok(access)
    }

    /// 在独立事务中证明创建资格。
    ///
    /// # 参数
    /// * `actor` - 已认证操作人，将写入首条主责
    ///
    /// # 返回
    /// 创建范围覆盖操作人自身主责时返回授权上下文。
    ///
    /// # 错误
    /// 无创建动作、范围为空或操作人没有有效主属组织时拒绝。
    ///
    /// # 关键业务约束
    /// 资料根命令与客户角色创建共用本检查，不得把请求中的负责销售当作授权。
    pub async fn ensure_create(&self, actor: &AuditActor) -> Result<AuthorizedDataScope> {
        let this = self.clone();
        let actor = actor.clone();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move { this.require_create(&actor, executor).await })
            })
            .await
    }

    /// 在调用方事务内证明创建资格。
    ///
    /// # 参数
    /// * `actor` - 已认证操作人，将写入首条主责
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 创建范围覆盖操作人自身主责时返回授权上下文。
    ///
    /// # 错误
    /// 无创建动作、范围为空或操作人没有有效主属组织时拒绝。
    ///
    /// # 关键业务约束
    /// 首条 OWNER 固定为创建人，请求中的负责销售字段不得成为授权依据。
    pub async fn require_create(
        &self,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<AuthorizedDataScope> {
        let (access, scope) = self.resolve(actor, "create", executor).await?;
        let owner_org = access.organizations.own_org(actor.id(), access.as_of)?;
        if !scope.allows_creation(actor.id(), owner_org) {
            return Err(Error::Forbidden("当前账号无权在授权范围内创建客户".into()));
        }
        Ok(access)
    }

    /// 读取动作已由身份域证明后，补充该账号全部历史归属客户。
    ///
    /// # 参数
    /// * `user` - 当前账号
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 返回去重后的历史参与客户 ID。
    ///
    /// # 错误
    /// 超限或数据库读取失败时拒绝。
    ///
    /// # 关键业务约束
    /// 历史参与必须来自归属事实，不得由创建人或业绩快照推导。
    async fn historical_customers(&self, user: &str, executor: &mut dyn Executor) -> Result<Vec<String>> {
        let mut ids = self
            .db
            .customer_assignments()
            .list_for_user(user, executor)
            .await?
            .into_iter()
            .map(|assignment| assignment.customer_id.to_string())
            .collect::<Vec<_>>();
        ids.sort();
        ids.dedup();
        ensure_limit(ids.len(), "历史参与范围超过查询上限")?;
        Ok(ids)
    }

    /// 按各角色组织目标批量解析当前主负责人所属组织对应的客户。
    ///
    /// # 参数
    /// * `access` - 已解析的身份范围
    /// * `as_of` - 客户归属自然日
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 返回「组织集合 → 当前主责客户」映射，键为排序后的组织 ID 列表。
    ///
    /// # 错误
    /// 成员或归属展开超限、组织树非法时拒绝。
    ///
    /// # 关键业务约束
    /// 组织筛选与授权都只看当前主负责人所属组织，不看协作人员组织。
    async fn org_owned_customers(
        &self,
        access: &AuthorizedDataScope,
        as_of: BusinessDate,
        executor: &mut dyn Executor,
    ) -> Result<Vec<(Vec<String>, Vec<String>)>> {
        let mut sets = Vec::new();
        for clause in access
            .scope
            .role_clauses
            .iter()
            .chain(access.scope.user_limit.iter())
        {
            if clause.org_unit_ids.is_empty() {
                continue;
            }
            let orgs = clause.org_unit_ids.iter().cloned().collect::<Vec<_>>();
            if sets.iter().any(|(existing, _)| existing == &orgs) {
                continue;
            }
            let customers = self
                .customers_owned_in_orgs(
                    &access.organizations,
                    &clause.org_unit_ids,
                    access.as_of,
                    as_of,
                    executor,
                )
                .await?;
            sets.push((orgs, customers));
        }
        Ok(sets)
    }

    /// 将内部组织展开为当前主属成员，再取其当前负责的客户。
    ///
    /// # 参数
    /// * `state` - 同一授权时点的组织事实
    /// * `org_ids` - 已由 DataScope 展开的内部组织
    /// * `at` - 授权时点，用于成员有效期
    /// * `as_of` - 客户归属自然日
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 返回这些组织当前成员作为主责的客户 ID。
    ///
    /// # 错误
    /// 成员或客户集合超过查询上限时拒绝。
    ///
    /// # 关键业务约束
    /// 空成员集保持空结果，不得查询全部客户。
    async fn customers_owned_in_orgs(
        &self,
        state: &OrganizationState,
        org_ids: &BTreeSet<String>,
        at: Instant,
        as_of: BusinessDate,
        executor: &mut dyn Executor,
    ) -> Result<Vec<String>> {
        let members = member_ids(state, org_ids, at);
        ensure_limit(members.len(), "组织成员超过查询上限")?;
        if members.is_empty() {
            return Ok(Vec::new());
        }
        let mut customers = self
            .db
            .customer_assignments()
            .current_owners(None, Some(&members), as_of, executor)
            .await?
            .into_iter()
            .map(|assignment| assignment.customer_id.to_string())
            .collect::<Vec<_>>();
        customers.sort();
        customers.dedup();
        ensure_limit(customers.len(), "组织主责客户超过查询上限")?;
        Ok(customers)
    }
}

/// 参与关系仅补充客户明确登记的读取动作。
///
/// # 参数
/// * `action` - 客户资源动作
///
/// # 返回
/// 列表和详情允许历史参与；创建、修改和删除不允许。
///
/// # 错误
/// 无。
///
/// # 关键业务约束
/// 历史参与不得为写命令或其他资源提供资格。
fn allows_history(action: &str) -> bool {
    matches!(action, "list" | "detail")
}

/// 客户自然日规则使用授权上下文的同一时点。
///
/// # 参数
/// * `at` - 授权解析时点
///
/// # 返回
/// 返回 Asia/Shanghai 自然日。
///
/// # 错误
/// 时点无法表示为业务日期时返回校验错误。
///
/// # 关键业务约束
/// 不改变客户归属的自然日合同，只避免查询跨上海零点时混用两天的资格。
pub(super) fn business_date(at: Instant) -> Result<BusinessDate> {
    let date = at.as_utc() + chrono::Duration::hours(8);
    Ok(date.format("%Y-%m-%d").to_string().parse()?)
}

/// 展开查询参数中的组织及其可选下级。
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
/// 未知组织拒绝；不得忽略后查询全部组织。
///
/// # 关键业务约束
/// 筛选只能收窄授权结果，展开失败必须返回校验错误。
pub(super) fn expand_org_filter(
    state: &OrganizationState,
    org_ids: &[String],
    include_descendants: bool,
) -> Result<BTreeSet<String>> {
    let tree = OrgTree::new(&state.units)?;
    let mut expanded = BTreeSet::new();
    for id in org_ids {
        expanded.extend(tree.expand(id, include_descendants)?);
    }
    Ok(expanded)
}

/// 映射同角色正向范围和独立个人上限。
///
/// # 参数
/// * `access` - 身份域解析结果
/// * `user` - 当前账号
/// * `owned` - 当前主责客户
/// * `collaborating` - 当前协作客户
/// * `history` - 历史参与客户
/// * `org_owned` - 各组织集合对应的当前主责客户
///
/// # 返回
/// 返回仓储可消费的客户授权条件。
///
/// # 错误
/// 无。
///
/// # 关键业务约束
/// 角色并集与个人上限分别保留；缺范围保持空集。
fn customer_scope(
    access: &AuthorizedDataScope,
    user: &str,
    owned: &[String],
    collaborating: &[String],
    history: Vec<String>,
    org_owned: &[(Vec<String>, Vec<String>)],
) -> CustomerReadScope {
    let roles = access
        .scope
        .role_clauses
        .iter()
        .map(|clause| map_clause(clause, user, collaborating, org_owned))
        .collect::<Vec<_>>();
    let user_limit = access
        .scope
        .user_limit
        .as_ref()
        .map(|clause| map_clause(clause, user, collaborating, org_owned));
    let mut authorized = union_clauses(&roles, owned, org_owned);
    if !history.is_empty() {
        authorized = union_ids(authorized, Some(history.clone()));
    }
    if let Some(limit) = &user_limit {
        authorized = intersect_ids(authorized, clause_ids(limit, owned, org_owned));
    }
    CustomerReadScope {
        roles,
        user_limit,
        historical_customer_ids: history,
        owned_customer_ids: owned.to_vec(),
        collaborative_customer_ids: collaborating.to_vec(),
        authorized_customer_ids: authorized,
    }
}

/// 将身份域条款映射为客户责任条款。
///
/// # 参数
/// * `clause` - 身份域正向范围
/// * `user` - 当前账号
/// * `collaborating` - 当前协作客户
/// * `org_owned` - 组织到客户的预计算结果
///
/// # 返回
/// 返回主责、协作与组织分别保留的客户条款。
///
/// # 错误
/// 无。
///
/// # 关键业务约束
/// 协作客户集合只在条款声明 Collaborative 时填入。
fn map_clause(
    clause: &ScopeClause,
    user: &str,
    collaborating: &[String],
    _org_owned: &[(Vec<String>, Vec<String>)],
) -> CustomerScopeClause {
    CustomerScopeClause {
        company: clause.company,
        owner_user_id: clause.self_owned.then(|| user.into()),
        collaborative_customer_ids: if clause.collaborative {
            collaborating.to_vec()
        } else {
            Vec::new()
        },
        owner_org_unit_ids: clause.org_unit_ids.iter().cloned().collect(),
    }
}

/// 计算单条条款覆盖的客户集合。
///
/// # 参数
/// * `clause` - 客户责任条款
/// * `owned` - 当前主责客户
/// * `org_owned` - 组织到客户的预计算结果
///
/// # 返回
/// 公司范围返回 `None`；否则返回并集 ID。
///
/// # 错误
/// 无。
///
/// # 关键业务约束
/// 主责、协作与组织在同一条款内按并集解释。
fn clause_ids(
    clause: &CustomerScopeClause,
    owned: &[String],
    org_owned: &[(Vec<String>, Vec<String>)],
) -> Option<Vec<String>> {
    if clause.company {
        return None;
    }
    let mut ids = Vec::new();
    if clause.owner_user_id.is_some() {
        ids.extend(owned.iter().cloned());
    }
    ids.extend(clause.collaborative_customer_ids.iter().cloned());
    if !clause.owner_org_unit_ids.is_empty() {
        if let Some((_, customers)) = org_owned
            .iter()
            .find(|(orgs, _)| orgs == &clause.owner_org_unit_ids)
        {
            ids.extend(customers.iter().cloned());
        }
    }
    ids.sort();
    ids.dedup();
    Some(ids)
}

/// 将角色条款求并为授权集合。
///
/// # 参数
/// * `roles` - 角色条款
/// * `owned` - 当前主责客户
/// * `org_owned` - 组织到客户的预计算结果
///
/// # 返回
/// 任一角色为公司范围时返回 `None`；没有角色时返回空集。
///
/// # 错误
/// 无。
///
/// # 关键业务约束
/// 缺角色范围不贡献对象，不能用公司范围兜底。
fn union_clauses(
    roles: &[CustomerScopeClause],
    owned: &[String],
    org_owned: &[(Vec<String>, Vec<String>)],
) -> Option<Vec<String>> {
    if roles.is_empty() {
        return Some(Vec::new());
    }
    let mut result: Option<Vec<String>> = Some(Vec::new());
    for clause in roles {
        result = union_ids(result, clause_ids(clause, owned, org_owned));
        if result.is_none() {
            return None;
        }
    }
    result
}

/// 两个授权集合按并集组合；`None` 表示公司范围。
///
/// # 参数
/// * `left` - 左侧集合
/// * `right` - 右侧集合
///
/// # 返回
/// 任一侧为公司范围则结果为公司范围。
///
/// # 错误
/// 无。
///
/// # 关键业务约束
/// 并集不得把空集解释为全量。
fn union_ids(left: Option<Vec<String>>, right: Option<Vec<String>>) -> Option<Vec<String>> {
    match (left, right) {
        (None, _) | (_, None) => None,
        (Some(mut left), Some(right)) => {
            left.extend(right);
            left.sort();
            left.dedup();
            Some(left)
        }
    }
}

/// 两个授权集合按交集收窄。
///
/// # 参数
/// * `left` - 左侧集合
/// * `right` - 右侧集合
///
/// # 返回
/// 任一侧为公司范围则结果为另一侧。
///
/// # 错误
/// 无。
///
/// # 关键业务约束
/// 个人上限与业务筛选只能收窄，不能扩大。
pub(super) fn intersect_ids(left: Option<Vec<String>>, right: Option<Vec<String>>) -> Option<Vec<String>> {
    match (left, right) {
        (None, other) | (other, None) => other,
        (Some(left), Some(right)) => {
            let right = right.into_iter().collect::<BTreeSet<_>>();
            Some(left.into_iter().filter(|id| right.contains(id)).collect())
        }
    }
}

/// 拒绝不可见对象且不泄露存在性差异。
///
/// # 参数
/// * `action` - 客户动作
///
/// # 返回
/// 读取返回 NotFound；写入返回 Forbidden。
///
/// # 错误
/// 无。
///
/// # 关键业务约束
/// 不可读详情不得暴露客户是否存在。
pub(super) fn deny_object(action: &str) -> Error {
    if matches!(action, "list" | "detail") {
        Error::NotFound("客户不存在或无权查看".into())
    } else {
        Error::Forbidden("当前用户不在该客户的授权范围内".into())
    }
}

/// 拒绝超过查询上限的展开结果。
///
/// # 参数
/// * `count` - 已展开数量
/// * `message` - 超限提示
///
/// # 返回
/// 未超限时返回 `Ok`。
///
/// # 错误
/// 超过 10000 时返回校验错误。
///
/// # 关键业务约束
/// 不得静默截断授权或筛选身份集合。
fn ensure_limit(count: usize, message: &str) -> Result<()> {
    if count > 10_000 {
        return Err(Error::ValidationError(format!("{message}，请收窄授权范围")));
    }
    Ok(())
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
pub(super) fn member_ids(state: &OrganizationState, org_ids: &BTreeSet<String>, at: Instant) -> Vec<String> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn historical_participation_never_grants_commands() {
        for action in ["list", "detail"] {
            assert!(allows_history(action));
        }
        for action in ["create", "update", "delete", "*"] {
            assert!(!allows_history(action));
        }
    }

    #[test]
    fn customer_qualification_uses_the_authorization_instant_at_shanghai_midnight() {
        let before = chrono::DateTime::parse_from_rfc3339("2026-09-13T15:59:59Z").unwrap();
        let after = chrono::DateTime::parse_from_rfc3339("2026-09-13T16:00:00Z").unwrap();
        assert_eq!(
            business_date(Instant::from_unix_secs(before.timestamp()))
                .unwrap()
                .to_string(),
            "2026-09-13"
        );
        assert_eq!(
            business_date(Instant::from_unix_secs(after.timestamp()))
                .unwrap()
                .to_string(),
            "2026-09-14"
        );
    }

    #[test]
    fn owner_and_collaborator_ids_are_unioned_per_clause_then_limited() {
        let owner = CustomerScopeClause {
            owner_user_id: Some("sales-a".into()),
            ..Default::default()
        };
        let collab = CustomerScopeClause {
            collaborative_customer_ids: vec!["c-collab".into()],
            ..Default::default()
        };
        let owned = vec!["c-own".to_string()];
        assert_eq!(clause_ids(&owner, &owned, &[]).as_deref(), Some(owned.as_slice()));
        assert_eq!(
            clause_ids(&collab, &owned, &[]),
            Some(vec!["c-collab".to_string()])
        );
        let both = CustomerScopeClause {
            owner_user_id: Some("sales-a".into()),
            collaborative_customer_ids: vec!["c-collab".into()],
            ..Default::default()
        };
        assert_eq!(
            clause_ids(&both, &owned, &[]).unwrap(),
            vec!["c-collab".to_string(), "c-own".to_string()]
        );
    }

    #[test]
    fn missing_role_scope_does_not_become_company() {
        assert_eq!(union_clauses(&[], &[], &[]), Some(Vec::<String>::new()));
        let company = CustomerScopeClause {
            company: true,
            ..Default::default()
        };
        assert!(union_clauses(&[company], &[], &[]).is_none());
    }
}
