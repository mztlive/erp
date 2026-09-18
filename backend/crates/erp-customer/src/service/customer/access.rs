//! 客户对象读取与写入范围；主责、协作与负责人组织分别解释。

use std::collections::BTreeSet;
use std::sync::Arc;

use application_core::AuditActor;
use erp_core::common::time::{BusinessDate, Instant};
use mongodb::Database;
use persistence_core::{Executor, Transactional};

use crate::entity::customer::AssignmentRole;
use crate::error::{Error, Result};
use crate::ports::{
    CustomerDataScopePort, CustomerResolvedClause, CustomerResolvedScope, CustomerScopeObject,
};
use crate::repository::CustomerExt;
use crate::repository::customer_shared::distinct_sorted_customer_ids;
use crate::repository::prelude::*;
use crate::repository::scope::{CustomerReadScope, CustomerScopeClause};

/// 客户对象访问范围；列表、详情、导出和写命令复用同一解析。
#[derive(Clone)]
pub struct CustomerAccess {
    db: Database,
    scope: Arc<dyn CustomerDataScopePort>,
}

impl CustomerAccess {
    /// 绑定客户归属事实来源与范围授权 Port。
    ///
    /// # 参数
    /// * `db` - 客户集合所在数据库
    /// * `scope` - 组合层注入的客户范围 Port
    ///
    /// # 返回
    /// 返回无授权缓存的访问服务，构造不执行 I/O。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// 不得在构造时补公司范围或读取登录人默认组织；不得直接构造身份域 Service。
    pub fn new(db: Database, scope: Arc<dyn CustomerDataScopePort>) -> Self {
        Self { db, scope }
    }

    /// 在调用方事务内证明资源动作并映射客户责任条件。
    ///
    /// # 参数
    /// * `actor` - 已认证操作人
    /// * `action` - 已注册的客户动作
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 返回已解析范围事实和客户授权条件。
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
    ) -> Result<(CustomerResolvedScope, CustomerReadScope)> {
        let mut access = self.scope.resolve(actor, action, executor).await?;
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
        let owned = distinct_sorted_customer_ids(owned);
        let collaborating = distinct_sorted_customer_ids(collaborating);
        let history = if allows_history(action) {
            self.historical_customers(actor.id(), executor).await?
        } else {
            Vec::new()
        };
        ensure_limit(owned.len(), "主责范围超过查询上限")?;
        ensure_limit(collaborating.len(), "协作范围超过查询上限")?;
        let org_owned = self.org_owned_customers(&access, as_of, executor).await?;
        access.scope_version = crate::service::customer::scope::fingerprint_scope_version(
            &access.scope_version,
            &owned,
            &collaborating,
            &history,
            &org_owned,
        );
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
    ) -> Result<CustomerResolvedScope> {
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
    ) -> Result<CustomerResolvedScope> {
        let (access, scope) = self.resolve(&actor, action, executor).await?;
        let found = self.db.customer_accounts().find_authorized(customer_id, &scope, executor).await?;
        if found.is_none() {
            return Err(deny_object(action));
        }
        let object = self.object_facts(customer_id, &access, &scope, executor).await?;
        if !self.scope.allows(&access, &object)? {
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
    pub async fn ensure_create(&self, actor: &AuditActor) -> Result<CustomerResolvedScope> {
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
    ) -> Result<CustomerResolvedScope> {
        let access = self.scope.resolve(actor, "create", executor).await?;
        let owner_org = self
            .scope
            .own_org(actor.id(), access.as_of, executor)
            .await?
            .ok_or_else(|| Error::ValidationError("请先维护负责人的有效主属组织".into()))?;
        let object = CustomerScopeObject { owned: true, org_unit_id: Some(owner_org), ..Default::default() };
        if !self.scope.allows(&access, &object)? {
            return Err(Error::Forbidden("当前账号无权在授权范围内创建客户".into()));
        }
        Ok(access)
    }

    /// 在相同执行器和授权时点取得客户当前主责组织，协作组织不参与映射。
    async fn object_facts(
        &self,
        id: &str,
        access: &CustomerResolvedScope,
        scope: &CustomerReadScope,
        executor: &mut dyn Executor,
    ) -> Result<CustomerScopeObject> {
        let owners = self
            .db
            .customer_assignments()
            .current_owners(Some(&[id.to_string()]), None, business_date(access.as_of)?, executor)
            .await?;
        let mut org_unit_id = None;
        if let Some(owner) = owners.first() {
            org_unit_id = self.scope.own_org(&owner.user_id, access.as_of, executor).await?;
        }
        Ok(CustomerScopeObject {
            owned: scope.owned_customer_ids.iter().any(|value| value == id),
            collaborating: scope.collaborative_customer_ids.iter().any(|value| value == id),
            historical_read_participant: scope.historical_customer_ids.iter().any(|value| value == id),
            org_unit_id,
        })
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
        let ids = self
            .db
            .customer_assignments()
            .list_for_user(user, executor)
            .await?
            .into_iter()
            .map(|assignment| assignment.customer_id.to_string())
            .collect::<Vec<_>>();
        let ids = distinct_sorted_customer_ids(ids);
        ensure_limit(ids.len(), "历史参与范围超过查询上限")?;
        Ok(ids)
    }

    /// 按各角色组织目标批量解析当前主负责人所属组织对应的客户。
    ///
    /// # 参数
    /// * `access` - 已解析的客户范围事实
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
        access: &CustomerResolvedScope,
        as_of: BusinessDate,
        executor: &mut dyn Executor,
    ) -> Result<Vec<(Vec<String>, Vec<String>)>> {
        let mut sets = Vec::new();
        for clause in access.role_clauses.iter().chain(access.user_limit.iter()) {
            if clause.org_unit_ids.is_empty() {
                continue;
            }
            let orgs = clause.org_unit_ids.clone();
            if sets.iter().any(|(existing, _)| existing == &orgs) {
                continue;
            }
            let org_ids = orgs.iter().cloned().collect::<BTreeSet<_>>();
            let members = self.scope.org_member_ids(&org_ids, access.as_of, executor).await?;
            ensure_limit(members.len(), "组织成员超过查询上限")?;
            let customers = self.customers_owned_by_members(&members, as_of, executor).await?;
            sets.push((orgs, customers));
        }
        Ok(sets)
    }

    /// 按已解析成员读取其当前负责的客户。
    ///
    /// # 参数
    /// * `members` - 组织在授权时点的有效主属成员
    /// * `as_of` - 客户归属自然日
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 返回这些成员作为主责的客户 ID。
    ///
    /// # 错误
    /// 客户集合超过查询上限时拒绝。
    ///
    /// # 关键业务约束
    /// 空成员集保持空结果，不得查询全部客户。
    async fn customers_owned_by_members(
        &self,
        members: &[String],
        as_of: BusinessDate,
        executor: &mut dyn Executor,
    ) -> Result<Vec<String>> {
        if members.is_empty() {
            return Ok(Vec::new());
        }
        let customers = self
            .db
            .customer_assignments()
            .current_owners(None, Some(members), as_of, executor)
            .await?
            .into_iter()
            .map(|assignment| assignment.customer_id.to_string())
            .collect::<Vec<_>>();
        let customers = distinct_sorted_customer_ids(customers);
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

/// 映射同角色正向范围和独立个人上限。
///
/// Port→仓储的唯一转换入口（erp-customer-005）：新增维度时只改此处与
/// [`map_clause`]，不得在 Service/Repository 另写逐字段搬运；两套条款的
/// `has_scope_rules/is_empty` 语义已对齐。
///
/// # 参数
/// * `access` - 已解析的客户范围事实
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
pub fn customer_scope(
    access: &CustomerResolvedScope,
    user: &str,
    owned: &[String],
    collaborating: &[String],
    history: Vec<String>,
    org_owned: &[(Vec<String>, Vec<String>)],
) -> CustomerReadScope {
    let roles = access
        .role_clauses
        .iter()
        .map(|clause| map_clause(clause, user, collaborating, org_owned))
        .collect::<Vec<_>>();
    let user_limit =
        access.user_limit.as_ref().map(|clause| map_clause(clause, user, collaborating, org_owned));
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

/// 将已解析条款映射为客户责任条款。
///
/// 唯一逐字段搬运点（erp-customer-005），由 [`customer_scope`] 统一调用。
///
/// # 参数
/// * `clause` - Port 返回的正向范围
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
    clause: &CustomerResolvedClause,
    user: &str,
    collaborating: &[String],
    _org_owned: &[(Vec<String>, Vec<String>)],
) -> CustomerScopeClause {
    CustomerScopeClause {
        company: clause.company,
        owner_user_id: clause.self_owned.then(|| user.into()),
        collaborative_customer_ids: if clause.collaborative { collaborating.to_vec() } else { Vec::new() },
        owner_org_unit_ids: clause.org_unit_ids.clone(),
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
    if !clause.owner_org_unit_ids.is_empty()
        && let Some((_, customers)) = org_owned.iter().find(|(orgs, _)| orgs == &clause.owner_org_unit_ids)
    {
        ids.extend(customers.iter().cloned());
    }
    Some(distinct_sorted_customer_ids(ids))
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
        result.as_ref()?;
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
            Some(distinct_sorted_customer_ids(left))
        },
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
        },
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
/// 列表与组织筛选共用同一上限（`scope` 复用本函数，避免各自硬编码 10000）。
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
pub(super) fn ensure_limit(count: usize, message: &str) -> Result<()> {
    if count > QUERY_LIMIT {
        return Err(Error::ValidationError(format!("{message}，请收窄授权范围")));
    }
    Ok(())
}

/// 授权与筛选集合的查询上限。
pub(super) const QUERY_LIMIT: usize = 10_000;

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
            business_date(Instant::from_unix_secs(before.timestamp())).unwrap().to_string(),
            "2026-09-13"
        );
        assert_eq!(
            business_date(Instant::from_unix_secs(after.timestamp())).unwrap().to_string(),
            "2026-09-14"
        );
    }

    #[test]
    fn owner_and_collaborator_ids_are_unioned_per_clause_then_limited() {
        let owner = CustomerScopeClause { owner_user_id: Some("sales-a".into()), ..Default::default() };
        let collab =
            CustomerScopeClause { collaborative_customer_ids: vec!["c-collab".into()], ..Default::default() };
        let owned = vec!["c-own".to_string()];
        assert_eq!(clause_ids(&owner, &owned, &[]).as_deref(), Some(owned.as_slice()));
        assert_eq!(clause_ids(&collab, &owned, &[]), Some(vec!["c-collab".to_string()]));
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
        let company = CustomerScopeClause { company: true, ..Default::default() };
        assert!(union_clauses(&[company], &[], &[]).is_none());
    }

    #[test]
    fn write_commands_deny_out_of_scope_as_forbidden() {
        assert!(matches!(deny_object("update"), Error::Forbidden(_)));
        assert!(matches!(deny_object("delete"), Error::Forbidden(_)));
        assert!(matches!(deny_object("create"), Error::Forbidden(_)));
        assert!(matches!(deny_object("detail"), Error::NotFound(_)));
    }

    #[test]
    fn query_limit_rejects_overflow() {
        assert!(ensure_limit(QUERY_LIMIT, "主责范围超过查询上限").is_ok());
        assert!(ensure_limit(QUERY_LIMIT + 1, "主责范围超过查询上限").is_err());
    }
}
