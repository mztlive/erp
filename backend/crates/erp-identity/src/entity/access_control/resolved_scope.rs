//! 按同一角色证明动作后的范围解析；角色授权与个人上限始终分别保留。

use erp_core::common::time::Instant;
use std::collections::BTreeSet;

use super::{DataScope, DataScopeSubjectType, DataScopeType, ScopeDimension, ScopeTargetMode};
use crate::entity::organization::{OrgManagementAssignment, OrgMembership, OrgTree};
use crate::Result;

#[cfg(test)]
#[path = "resolved_scope_tests.rs"]
mod tests;

/// 同一角色在单个资源动作上的正向范围；不同维度按交集解释。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ScopeClause {
    pub company: bool,
    pub self_owned: bool,
    pub collaborative: bool,
    pub org_unit_ids: BTreeSet<String>,
    pub settlement_party_ids: BTreeSet<String>,
    pub warehouse_ids: BTreeSet<String>,
}

/// 已经通过账号、角色状态和完整动作权限校验的解析输入。
pub struct ScopeResolution<'a> {
    pub user_id: &'a str,
    pub eligible_role_ids: &'a [String],
    pub resource: &'a str,
    pub action: &'a str,
    pub required_dimensions: &'a [ScopeDimension],
    pub rules: &'a [DataScope],
    pub memberships: &'a [OrgMembership],
    pub management: &'a [OrgManagementAssignment],
    pub tree: &'a OrgTree<'a>,
    pub as_of: Instant,
}

/// 范围解析结果不直接序列化给客户端。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedScope {
    pub role_clauses: Vec<ScopeClause>,
    pub user_limit: Option<ScopeClause>,
}

/// 业务域提供的对象责任事实；历史归属快照不得作为参与事实输入。
pub struct ScopedObject<'a> {
    pub owned: bool,
    pub collaborating: bool,
    pub historical_read_participant: bool,
    pub org_unit_id: Option<&'a str>,
    pub settlement_party_id: Option<&'a str>,
    pub warehouse_id: Option<&'a str>,
}

impl ScopeClause {
    /// 判断对象是否满足该角色的各适用维度。
    ///
    /// # 返回
    /// Company 覆盖维度；普通范围逐维求交，空正向范围不贡献对象。
    pub fn covers(&self, object: &ScopedObject<'_>) -> bool {
        if self.company {
            return true;
        }
        let ownership = (self.self_owned && object.owned)
            || (self.collaborative && object.collaborating)
            || object
                .org_unit_id
                .is_some_and(|id| self.org_unit_ids.contains(id));
        let has_ownership_dimension = self.self_owned || self.collaborative || !self.org_unit_ids.is_empty();
        let has_other_dimension = !self.settlement_party_ids.is_empty() || !self.warehouse_ids.is_empty();
        (!has_ownership_dimension || ownership)
            && (has_ownership_dimension || has_other_dimension)
            && (self.settlement_party_ids.is_empty()
                || object
                    .settlement_party_id
                    .is_some_and(|id| self.settlement_party_ids.contains(id)))
            && (self.warehouse_ids.is_empty()
                || object
                    .warehouse_id
                    .is_some_and(|id| self.warehouse_ids.contains(id)))
    }

    /// 校验资源要求的维度均有该角色自己的范围证据。
    fn complete(&self, dimensions: &[ScopeDimension]) -> bool {
        self.company
            || dimensions.iter().all(|dimension| match dimension {
                ScopeDimension::InternalOrg => {
                    self.self_owned || self.collaborative || !self.org_unit_ids.is_empty()
                }
                ScopeDimension::SettlementParty => !self.settlement_party_ids.is_empty(),
                ScopeDimension::Warehouse => !self.warehouse_ids.is_empty(),
            })
    }
}

impl ResolvedScope {
    /// 判断读取或写入范围；调用方必须先证明有效账号和完整动作权限。
    ///
    /// # 返回
    /// 合法历史参与仅补读取，仍与用户上限求交；角色缺范围不影响合法参与。
    pub fn allows(&self, object: &ScopedObject<'_>, allow_history: bool) -> bool {
        let granted = self.role_clauses.iter().any(|scope| scope.covers(object))
            || (allow_history && object.historical_read_participant);
        granted && self.user_limit.as_ref().is_none_or(|limit| limit.covers(object))
    }
}

impl ScopeResolution<'_> {
    /// 解析角色并集及用户上限；输入角色必须由现有 RBAC 同角色完整权限检查产生。
    ///
    /// # 错误
    /// 无效目标或组织关系拒绝；缺范围、缺组织和缺必需维度保持空集。
    pub fn resolve(&self) -> Result<ResolvedScope> {
        if self.eligible_role_ids.is_empty() {
            return Err(crate::Error::Forbidden("没有提供完整动作权限的有效角色".into()));
        }
        let mut role_clauses = Vec::new();
        for role_id in self.eligible_role_ids {
            let rules = self.rules_for(DataScopeSubjectType::Role, role_id);
            let clause = self.clause(&rules, Some(role_id))?;
            if clause.complete(self.required_dimensions) {
                role_clauses.push(clause);
            }
        }
        let rules = self.rules_for(DataScopeSubjectType::User, self.user_id);
        let user_limit = (!rules.is_empty())
            .then(|| self.clause(&rules, None))
            .transpose()?;
        Ok(ResolvedScope {
            role_clauses,
            user_limit,
        })
    }

    /// 只提取当前资源动作、主体、状态均匹配的规则。
    fn rules_for(&self, subject_type: DataScopeSubjectType, id: &str) -> Vec<&DataScope> {
        self.rules
            .iter()
            .filter(|rule| {
                !rule.base.is_deleted()
                    && rule.subject_type == subject_type
                    && rule.subject_id == id
                    && rule.binding.applies(self.resource, self.action)
            })
            .collect()
    }

    /// 汇总同一主体的同维度正向范围。
    fn clause(&self, rules: &[&DataScope], role_id: Option<&str>) -> Result<ScopeClause> {
        let mut clause = ScopeClause::default();
        for rule in rules {
            rule.binding.validate(rule.scope_type, &rule.scope_targets)?;
            match rule.scope_type {
                DataScopeType::Company => clause.company = true,
                DataScopeType::SelfOwned => clause.self_owned = true,
                DataScopeType::Collaborative => clause.collaborative = true,
                DataScopeType::Organization | DataScopeType::Team => {
                    self.add_targets(&mut clause, rule, role_id)?
                }
            }
        }
        Ok(clause)
    }

    /// 按类型添加目标，不将内部组织身份写入仓库或结算主体集合。
    fn add_targets(&self, clause: &mut ScopeClause, rule: &DataScope, role_id: Option<&str>) -> Result<()> {
        match rule.binding.target_dimension {
            ScopeDimension::InternalOrg => clause.org_unit_ids.extend(self.org_targets(rule, role_id)?),
            ScopeDimension::SettlementParty => clause
                .settlement_party_ids
                .extend(rule.scope_targets.iter().cloned()),
            ScopeDimension::Warehouse => clause.warehouse_ids.extend(rule.scope_targets.iter().cloned()),
        }
        Ok(())
    }

    /// 解析动态组织，管理关系只能由自身绑定的合格角色激活。
    fn org_targets(&self, rule: &DataScope, role_id: Option<&str>) -> Result<BTreeSet<String>> {
        let mut result = BTreeSet::new();
        let descendants = rule.binding.include_descendants.unwrap_or(false);
        match rule.binding.target_mode {
            Some(ScopeTargetMode::Explicit) => {
                for id in &rule.scope_targets {
                    result.extend(self.tree.expand(id, descendants)?);
                }
            }
            Some(ScopeTargetMode::OwnOrg) => {
                for membership in self.memberships.iter().filter(|m| {
                    !m.base.is_deleted() && m.user_id == self.user_id && m.validity.contains(self.as_of)
                }) {
                    result.extend(self.tree.expand(&membership.org_unit_id, descendants)?);
                }
            }
            Some(ScopeTargetMode::ManagedOrgs) => {
                for grant in self.management.iter().filter(|g| {
                    !g.base.is_deleted() && g.user_id == self.user_id && g.validity.contains(self.as_of)
                }) {
                    if self.eligible_role_ids.contains(&grant.role_id)
                        && role_id.is_none_or(|id| id == grant.role_id)
                    {
                        result.extend(self.tree.expand(&grant.org_unit_id, grant.include_descendants)?);
                    }
                }
            }
            None => {}
        }
        Ok(result)
    }
}
