//! 合同对象读取与写入范围；当前客户主责、协作与负责人组织分别解释。

use std::collections::BTreeSet;
use std::sync::Arc;

use application_core::AuditActor;
use erp_core::common::time::{BusinessDate, Instant};
use mongodb::Database;
use persistence_core::{Executor, Transactional};

use crate::error::{Error, Result};
use crate::ports::{
    ContractDataScopePort, ContractParticipantPort, ContractResolvedClause, ContractResolvedScope,
    ContractScopeObject, CustomerAssignmentFactsPort,
};
use crate::repository::ContractExt;
use crate::repository::scope::{ContractReadScope, ContractScopeClause};

/// 合同对象访问范围；列表、详情、附件、导出和写命令复用同一解析。
#[derive(Clone)]
pub struct ContractAccess {
    db: Database,
    scope: Arc<dyn ContractDataScopePort>,
    assignments: Arc<dyn CustomerAssignmentFactsPort>,
    participants: Arc<dyn ContractParticipantPort>,
}

impl ContractAccess {
    /// 绑定合同范围授权、客户归属与合法参与 Port。
    ///
    /// # 参数
    /// * `db` - 合同集合所在数据库
    /// * `scope` - 组合层注入的合同范围 Port
    /// * `assignments` - 当前客户主责与协作事实
    /// * `participants` - 合法单据参与事实
    ///
    /// # 返回
    /// 返回无授权缓存的访问服务，构造不执行 I/O。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// 不得在构造时补公司范围；不得用签约经办充当当前负责人。
    pub fn new(
        db: Database,
        scope: Arc<dyn ContractDataScopePort>,
        assignments: Arc<dyn CustomerAssignmentFactsPort>,
        participants: Arc<dyn ContractParticipantPort>,
    ) -> Self {
        Self { db, scope, assignments, participants }
    }

    /// 在调用方事务内证明资源动作并映射合同责任条件。
    ///
    /// # 参数
    /// * `actor` - 已认证操作人
    /// * `action` - 已注册的合同动作
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 返回已解析范围事实和合同授权条件。
    ///
    /// # 错误
    /// 无动作权限返回 Forbidden；查询超限或组织关系非法时拒绝。
    ///
    /// # 关键业务约束
    /// 当前跟进按客户当前主负责人解释；历史参与只补充读取。
    pub async fn resolve(
        &self,
        actor: &AuditActor,
        action: &str,
        executor: &mut dyn Executor,
    ) -> Result<(ContractResolvedScope, ContractReadScope)> {
        let mut access = self.scope.resolve(actor, action, executor).await?;
        let as_of = business_date(access.as_of)?;
        let assignments = self.assignments.active_assignments_for_user(actor.id(), as_of, executor).await?;
        let mut owned = Vec::new();
        let mut collaborating = Vec::new();
        for assignment in assignments {
            if assignment.is_owner {
                owned.push(assignment.customer_id);
            } else {
                collaborating.push(assignment.customer_id);
            }
        }
        owned.sort();
        owned.dedup();
        collaborating.sort();
        collaborating.dedup();
        ensure_limit(owned.len(), "主责范围超过查询上限")?;
        ensure_limit(collaborating.len(), "协作范围超过查询上限")?;
        let org_owned = self.org_owned_customers(&access, as_of, executor).await?;
        let limit_customers = access.user_limit.as_ref().and_then(|clause| {
            clause_ids(&map_clause(clause, actor.id(), &collaborating, &org_owned), &owned, &org_owned)
        });
        let history = if allows_history(action) {
            self.historical_contracts(actor.id(), limit_customers.as_deref(), executor).await?
        } else {
            Vec::new()
        };
        let fingerprint = scope_fingerprint_input(&owned, &collaborating, history.as_slice(), &org_owned);
        access.scope_version = format!("{}:{:x}", access.scope_version, stable_fingerprint(&fingerprint));
        let scope = contract_scope(&access, actor.id(), &owned, &collaborating, history, &org_owned);
        Ok((access, scope))
    }

    /// 在独立事务中重验对象资格。
    ///
    /// # 参数
    /// * `actor` - 已认证操作人
    /// * `action` - 已注册的合同动作
    /// * `contract_id` - 目标合同
    ///
    /// # 返回
    /// 对象在范围内时返回授权上下文。
    ///
    /// # 错误
    /// 读取动作对不可见对象返回 NotFound；写动作返回 Forbidden。
    ///
    /// # 关键业务约束
    /// 列表已授权不能作为详情、附件或命令的长期凭证；历史参与不授予修改。
    pub async fn require(
        &self,
        actor: &AuditActor,
        action: &str,
        contract_id: &str,
    ) -> Result<ContractResolvedScope> {
        let this = self.clone();
        let actor = actor.clone();
        let action = action.to_string();
        let contract_id = contract_id.to_string();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move { this.require_with(actor, &action, &contract_id, executor).await })
            })
            .await
    }

    /// 在调用方事务内重验对象资格。
    ///
    /// # 参数
    /// * `actor` - 已认证操作人
    /// * `action` - 已注册的合同动作
    /// * `contract_id` - 目标合同
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
        contract_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<ContractResolvedScope> {
        let (access, scope) = self.resolve(&actor, action, executor).await?;
        let found = self.db.contracts().find_authorized(contract_id, &scope, executor).await?;
        let contract = found.ok_or_else(|| deny_object(action))?;
        let mut object = self.object_facts(contract.customer_id.as_ref(), &access, &scope, executor).await?;
        object.historical_read_participant = scope.historical_contract_ids.iter().any(|id| id == contract_id);
        if !self.scope.allows(&access, &object)? {
            return Err(deny_object(action));
        }
        Ok(access)
    }

    /// 在调用方事务内证明按指定客户创建合同的资格。
    ///
    /// # 参数
    /// * `actor` - 已认证操作人
    /// * `customer_id` - 拟归档合同的客户
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 创建范围覆盖该客户时返回授权上下文。
    ///
    /// # 错误
    /// 无创建动作、范围为空或客户不在授权内时拒绝。
    ///
    /// # 关键业务约束
    /// 历史参与与签约经办不得单独放行建档。
    pub async fn require_create(
        &self,
        actor: &AuditActor,
        customer_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<ContractResolvedScope> {
        let (access, scope) = self.resolve(actor, "create", executor).await?;
        let object = self.object_facts(customer_id, &access, &scope, executor).await?;
        if !self.scope.allows(&access, &object)? {
            return Err(Error::Forbidden("当前账号无权在授权范围内归档合同".into()));
        }
        Ok(access)
    }

    /// 合同对象使用客户当前主负责人组织，不以签约人或上传人作为责任兜底。
    async fn object_facts(
        &self,
        id: &str,
        access: &ContractResolvedScope,
        scope: &ContractReadScope,
        executor: &mut dyn Executor,
    ) -> Result<ContractScopeObject> {
        let owners = self
            .assignments
            .owner_user_ids_by_customer(&[id.to_string()], business_date(access.as_of)?, executor)
            .await?;
        let mut org_unit_id = None;
        if let Some(owner) = owners.get(id) {
            org_unit_id = self.scope.own_org(owner, access.as_of, executor).await?;
        }
        Ok(ContractScopeObject {
            owned: scope.owned_customer_ids.iter().any(|value| value == id),
            collaborating: scope.collaborative_customer_ids.iter().any(|value| value == id),
            historical_read_participant: false,
            org_unit_id,
        })
    }

    /// 读取动作已由身份域证明后，补充该账号合法参与且仍存在的合同。
    ///
    /// # 参数
    /// * `user` - 当前账号
    /// * `limit_customers` - 个人上限客户集合；`None` 表示不附加个人限制
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 返回去重后的历史参与合同 ID。
    ///
    /// # 错误
    /// 超限或数据库读取失败时拒绝。
    ///
    /// # 关键业务约束
    /// 不得由创建人、签约日期或业绩快照推导参与资格；结果仍受个人上限限制。
    async fn historical_contracts(
        &self,
        user: &str,
        limit_customers: Option<&[String]>,
        executor: &mut dyn Executor,
    ) -> Result<Vec<String>> {
        let ids = self.participants.document_ids_by_user(user, executor).await?;
        ensure_limit(ids.len(), "历史参与范围超过查询上限")?;
        let refs = self.db.contracts().customer_refs_by_ids(&ids, executor).await?;
        ensure_limit(refs.len(), "历史参与合同超过查询上限")?;
        let mut contracts = refs
            .into_iter()
            .filter(|row| {
                limit_customers.is_none_or(|customers| customers.iter().any(|id| id == &row.customer_id))
            })
            .map(|row| row.id)
            .collect::<Vec<_>>();
        contracts.sort();
        contracts.dedup();
        Ok(contracts)
    }

    /// 按各角色组织目标批量解析当前主负责人所属组织对应的客户。
    ///
    /// # 参数
    /// * `access` - 已解析的合同范围事实
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
        access: &ContractResolvedScope,
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
        let mut customers =
            self.assignments.current_owner_customer_ids(None, Some(members), as_of, executor).await?;
        customers.sort();
        customers.dedup();
        ensure_limit(customers.len(), "组织主责客户超过查询上限")?;
        Ok(customers)
    }
}

/// 参与关系仅补充合同明确登记的读取动作。
///
/// # 参数
/// * `action` - 合同资源动作
///
/// # 返回
/// 列表和详情允许历史参与；创建、修改不允许。
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
/// # 参数
/// * `access` - 已解析的合同范围事实
/// * `user` - 当前账号
/// * `owned` - 当前主责客户
/// * `collaborating` - 当前协作客户
/// * `history` - 历史参与合同
/// * `org_owned` - 各组织集合对应的当前主责客户
///
/// # 返回
/// 返回仓储可消费的合同授权条件。
///
/// # 错误
/// 无。
///
/// # 关键业务约束
/// 角色并集与个人上限分别保留；缺范围保持空集；历史参与不并入写资格。
pub fn contract_scope(
    access: &ContractResolvedScope,
    user: &str,
    owned: &[String],
    collaborating: &[String],
    history: Vec<String>,
    org_owned: &[(Vec<String>, Vec<String>)],
) -> ContractReadScope {
    let roles = access
        .role_clauses
        .iter()
        .map(|clause| map_clause(clause, user, collaborating, org_owned))
        .collect::<Vec<_>>();
    let user_limit =
        access.user_limit.as_ref().map(|clause| map_clause(clause, user, collaborating, org_owned));
    let mut authorized = union_clauses(&roles, owned, org_owned);
    if let Some(limit) = &user_limit {
        authorized = intersect_ids(authorized, clause_ids(limit, owned, org_owned));
    }
    ContractReadScope {
        roles,
        user_limit,
        historical_contract_ids: history,
        owned_customer_ids: owned.to_vec(),
        collaborative_customer_ids: collaborating.to_vec(),
        authorized_customer_ids: authorized,
    }
}

/// 将已解析条款映射为合同责任条款（组织客户展开仍在 `clause_ids` 按组织回查）。
///
/// # 参数
/// * `clause` - Port 返回的正向范围
/// * `user` - 当前账号
/// * `collaborating` - 当前协作客户
/// * `org_owned` - 组织到客户的预计算结果（本函数只透传，展开在 `clause_ids` 内回查）
///
/// # 返回
/// 返回主责、协作与组织分别保留的合同条款。
///
/// # 错误
/// 无。
///
/// # 关键业务约束
/// 协作客户集合只在条款声明 Collaborative 时填入。
fn map_clause(
    clause: &ContractResolvedClause,
    user: &str,
    collaborating: &[String],
    org_owned: &[(Vec<String>, Vec<String>)],
) -> ContractScopeClause {
    ContractScopeClause {
        company: clause.company,
        owner_user_id: clause.self_owned.then(|| user.into()),
        collaborative_customer_ids: if clause.collaborative { collaborating.to_vec() } else { Vec::new() },
        owner_org_unit_ids: clause.org_unit_ids.clone(),
    }
}

/// 计算单条条款覆盖的客户集合。
///
/// # 参数
/// * `clause` - 合同责任条款
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
    clause: &ContractScopeClause,
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
    roles: &[ContractScopeClause],
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
            left.sort();
            left.dedup();
            Some(left)
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
/// * `action` - 合同动作
///
/// # 返回
/// 读取返回 NotFound；写入返回 Forbidden。
///
/// # 错误
/// 无。
///
/// # 关键业务约束
/// 不可读详情不得暴露合同是否存在。
pub(super) fn deny_object(action: &str) -> Error {
    if matches!(action, "list" | "detail") {
        Error::NotFound("合同不存在或无权查看".into())
    } else {
        Error::Forbidden("当前用户不在该合同的授权范围内".into())
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

/// 范围指纹的稳定序列化输入：排序后 ID 做长度前缀拼接（跨工具链稳定）。
///
/// # 参数
/// * `owned` - 当前主责客户
/// * `collaborating` - 当前协作客户
/// * `history` - 历史参与合同
/// * `org_owned` - 各组织集合对应的当前主责客户
///
/// # 返回
/// 返回带长度前缀的规范字节串，仅用于同版本内快照比对，不做持久化字段。
pub(super) fn scope_fingerprint_input(
    owned: &[String],
    collaborating: &[String],
    history: impl ScopeFingerprintHistory,
    org_owned: &[(Vec<String>, Vec<String>)],
) -> Vec<u8> {
    let mut out = Vec::new();
    push_str_section(&mut out, owned);
    push_str_section(&mut out, collaborating);
    history.push_section(&mut out);
    let mut orgs: Vec<(&[String], &[String])> =
        org_owned.iter().map(|(orgs, customers)| (orgs.as_slice(), customers.as_slice())).collect();
    orgs.sort();
    for (org_set, customers) in orgs {
        push_str_section(&mut out, org_set);
        push_str_section(&mut out, customers);
    }
    out
}

/// 范围指纹历史段的抽象：字符串列表或合同版本列表统一压入同一规范字节串。
pub(super) trait ScopeFingerprintHistory {
    /// 把历史段按规范编码追加到 `out`。
    fn push_section(self, out: &mut Vec<u8>);
}

impl ScopeFingerprintHistory for &[String] {
    fn push_section(self, out: &mut Vec<u8>) {
        let mut sorted: Vec<&str> = self.iter().map(String::as_str).collect();
        sorted.sort_unstable();
        push_str_section(out, &sorted);
    }
}

impl ScopeFingerprintHistory for &[crate::repository::scope::ContractVersion] {
    fn push_section(self, out: &mut Vec<u8>) {
        let mut pairs: Vec<(&str, u64)> =
            self.iter().map(|version| (version.id.as_str(), version.version)).collect();
        pairs.sort_unstable();
        push_u64(out, pairs.len() as u64);
        for (id, version) in pairs {
            push_bytes(out, id.as_bytes());
            push_u64(out, version);
        }
    }
}

fn push_str_section(out: &mut Vec<u8>, values: &[impl AsRef<str>]) {
    let mut sorted: Vec<&str> = values.iter().map(AsRef::as_ref).collect();
    sorted.sort_unstable();
    push_u64(out, sorted.len() as u64);
    for value in sorted {
        push_bytes(out, value.as_bytes());
    }
}

fn push_bytes(out: &mut Vec<u8>, bytes: &[u8]) {
    push_u64(out, bytes.len() as u64);
    out.extend_from_slice(bytes);
}

fn push_u64(out: &mut Vec<u8>, value: u64) {
    out.extend_from_slice(&value.to_le_bytes());
}

/// 对规范字节串计算稳定指纹（FNV-1a 64 位固定算法，不随工具链漂移）。
///
/// # 参数
/// * `input` - `scope_fingerprint_input` 输出的规范字节串
///
/// # 返回
/// 返回 64 位指纹，仅用于同版本内快照比对。
pub(super) fn stable_fingerprint(input: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in input {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn historical_participation_never_grants_commands() {
        for action in ["list", "detail"] {
            assert!(allows_history(action));
        }
        for action in ["create", "update", "*"] {
            assert!(!allows_history(action));
        }
    }

    #[test]
    fn contract_qualification_uses_the_authorization_instant_at_shanghai_midnight() {
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
        let owner = ContractScopeClause { owner_user_id: Some("sales-a".into()), ..Default::default() };
        let collab =
            ContractScopeClause { collaborative_customer_ids: vec!["c-collab".into()], ..Default::default() };
        let owned = vec!["c-own".to_string()];
        assert_eq!(clause_ids(&owner, &owned, &[]).as_deref(), Some(owned.as_slice()));
        assert_eq!(clause_ids(&collab, &owned, &[]), Some(vec!["c-collab".to_string()]));
    }

    #[test]
    fn missing_role_scope_does_not_become_company() {
        assert_eq!(union_clauses(&[], &[], &[]), Some(Vec::<String>::new()));
        let company = ContractScopeClause { company: true, ..Default::default() };
        assert!(union_clauses(&[company], &[], &[]).is_none());
    }

    #[test]
    fn history_is_not_merged_into_write_authorized_customers() {
        let access = ContractResolvedScope {
            user_id: "u1".into(),
            resource: "contract".into(),
            action: "list".into(),
            role_clauses: vec![],
            user_limit: None,
            policy_version: 1,
            organization_version: 1,
            scope_version: "v1".into(),
            as_of: Instant::from_unix_secs(0),
        };
        let scope = contract_scope(&access, "u1", &[], &[], vec!["ht-old".into()], &[]);
        assert_eq!(scope.authorized_customer_ids, Some(Vec::new()));
        assert_eq!(scope.historical_contract_ids, vec!["ht-old".to_string()]);
        assert!(!scope.allows_creation("cust-1"));
    }

    #[test]
    fn write_commands_deny_out_of_scope_as_forbidden() {
        assert!(matches!(deny_object("update"), Error::Forbidden(_)));
        assert!(matches!(deny_object("create"), Error::Forbidden(_)));
        assert!(matches!(deny_object("detail"), Error::NotFound(_)));
    }

    #[test]
    fn scope_fingerprint_is_order_insensitive_and_stable_across_runs() {
        let base = scope_fingerprint_input(
            &["c-2".to_string(), "c-1".to_string()],
            &["c-3".to_string()],
            ["ht-2".to_string(), "ht-1".to_string()].as_slice(),
            &[(vec!["org-b".to_string(), "org-a".to_string()], vec!["c-9".to_string(), "c-8".to_string()])],
        );
        let reordered = scope_fingerprint_input(
            &["c-1".to_string(), "c-2".to_string()],
            &["c-3".to_string()],
            ["ht-1".to_string(), "ht-2".to_string()].as_slice(),
            &[(vec!["org-a".to_string(), "org-b".to_string()], vec!["c-8".to_string(), "c-9".to_string()])],
        );
        assert_eq!(base, reordered);
        assert_eq!(stable_fingerprint(&base), stable_fingerprint(&reordered));
        let changed = scope_fingerprint_input(
            &["c-1".to_string()],
            &["c-3".to_string()],
            ["ht-1".to_string(), "ht-2".to_string()].as_slice(),
            &[],
        );
        assert_ne!(stable_fingerprint(&base), stable_fingerprint(&changed));
    }

    #[test]
    fn assignment_fact_does_not_treat_signer_as_owner() {
        let collab = crate::ports::ContractAssignmentFact {
            customer_id: "c-1".into(),
            user_id: "signer".into(),
            is_owner: false,
        };
        assert!(!collab.is_owner);
    }
}
