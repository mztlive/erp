use std::collections::BTreeSet;

use super::{FactKind, RelationKey, RelationMatch};
use crate::entity::access_control::{ResolvedScope, ScopeClause, ScopedObject};
use crate::entity::resource_access::ResourceKind;

/// 相对于本次授权账号的业务事实，账号由授权上下文固定。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AccessFact {
    Owned,
    Collaborating,
    OrganizationIn(BTreeSet<String>),
    SettlementPartyIn(BTreeSet<String>),
    WarehouseIn(BTreeSet<String>),
    HistoryParticipant,
    AssignedTo,
    Participant,
}

/// 只能由身份域构造；查询和对象判定共用的规则树。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum AccessRule {
    Allow,
    Deny,
    All(Vec<Self>),
    Any(Vec<Self>),
    Fact(AccessFact),
    Required { facts: BTreeSet<FactKind>, rule: Box<Self> },
    RecordReadGrant(ResourceKind),
    Related { relation: RelationKey, resource: String, mode: RelationMatch, rule: Box<Self> },
}

impl AccessRule {
    /// 归一化交集；空规则拒绝，显式 Allow 才表示无限制。
    pub(crate) fn all(rules: Vec<Self>) -> Self {
        if rules.is_empty() || rules.contains(&Self::Deny) {
            return Self::Deny;
        }
        let mut rules: Vec<_> = rules.into_iter().filter(|rule| *rule != Self::Allow).collect();
        match rules.len() {
            0 => Self::Allow,
            1 => rules.pop().expect("已验证只有一条规则"),
            _ => Self::All(rules),
        }
    }

    /// 归一化并集；空集合保持拒绝。
    pub(crate) fn any(rules: Vec<Self>) -> Self {
        if rules.contains(&Self::Allow) && !rules.iter().any(Self::has_requirements) {
            return Self::Allow;
        }
        let mut rules: Vec<_> = rules.into_iter().filter(|rule| *rule != Self::Deny).collect();
        match rules.len() {
            0 => Self::Deny,
            1 => rules.pop().expect("已验证只有一条规则"),
            _ => Self::Any(rules),
        }
    }

    /// 显式必需事实不能被无条件放行的并集分支删除。
    fn has_requirements(&self) -> bool {
        match self {
            Self::Required { facts, rule } => !facts.is_empty() || rule.has_requirements(),
            Self::All(items) | Self::Any(items) => items.iter().any(Self::has_requirements),
            Self::Related { rule, .. } => rule.has_requirements(),
            _ => false,
        }
    }

    /// 附加完整性检查，空声明不增加节点。
    pub(crate) fn requiring(self, facts: &BTreeSet<FactKind>) -> Self {
        if facts.is_empty() { self } else { Self::Required { facts: facts.clone(), rule: Box::new(self) } }
    }

    /// 保留历史参与与个人上限的位置，不把上限当作正向授权。
    pub(crate) fn scope(scope: &ResolvedScope, allow_history: bool) -> Self {
        let mut grants: Vec<_> = scope.role_clauses.iter().map(Self::clause).collect();
        if allow_history {
            grants.push(Self::Fact(AccessFact::HistoryParticipant));
        }
        let granted = Self::any(grants);
        match &scope.user_limit {
            Some(limit) => Self::all(vec![granted, Self::clause(limit)]),
            None => granted,
        }
    }

    /// 条款内归属维度取并集，各独立维度取交集。
    pub(crate) fn clause(clause: &ScopeClause) -> Self {
        if clause.company {
            return Self::Allow;
        }
        let mut ownership = Vec::new();
        if clause.self_owned {
            ownership.push(Self::Fact(AccessFact::Owned));
        }
        if clause.collaborative {
            ownership.push(Self::Fact(AccessFact::Collaborating));
        }
        if !clause.org_unit_ids.is_empty() {
            ownership.push(Self::Fact(AccessFact::OrganizationIn(clause.org_unit_ids.clone())));
        }
        let mut dimensions = Vec::new();
        if !ownership.is_empty() {
            dimensions.push(Self::any(ownership));
        }
        if !clause.settlement_party_ids.is_empty() {
            dimensions.push(Self::Fact(AccessFact::SettlementPartyIn(clause.settlement_party_ids.clone())));
        }
        if !clause.warehouse_ids.is_empty() {
            dimensions.push(Self::Fact(AccessFact::WarehouseIn(clause.warehouse_ids.clone())));
        }
        Self::all(dimensions)
    }

    /// 对象事实已经由拥有领域按授权账号与时点准备。
    pub(crate) fn allows_scope(&self, object: &ScopedObject<'_>) -> bool {
        self.evaluate(object, &|_| false)
    }

    /// 只有经过有效来源校验的读取资格可以满足记录授权节点。
    pub(crate) fn evaluate(
        &self,
        object: &ScopedObject<'_>,
        granted: &impl Fn(ResourceKind) -> bool,
    ) -> bool {
        match self {
            Self::Allow => true,
            Self::Deny => false,
            Self::All(rules) => !rules.is_empty() && rules.iter().all(|rule| rule.evaluate(object, granted)),
            Self::Any(rules) => rules.iter().any(|rule| rule.evaluate(object, granted)),
            Self::Fact(fact) => fact.matches_scope(object),
            Self::RecordReadGrant(resource) => granted(*resource),
            Self::Related { .. } | Self::Required { .. } => false,
        }
    }
}

impl AccessFact {
    /// 只判断本事实，不改变上层授权组合。
    fn matches_scope(&self, object: &ScopedObject<'_>) -> bool {
        match self {
            Self::Owned => object.owned,
            Self::Collaborating => object.collaborating,
            Self::OrganizationIn(ids) => object.org_unit_id.is_some_and(|id| ids.contains(id)),
            Self::SettlementPartyIn(ids) => object.settlement_party_id.is_some_and(|id| ids.contains(id)),
            Self::WarehouseIn(ids) => object.warehouse_id.is_some_and(|id| ids.contains(id)),
            Self::HistoryParticipant => object.historical_read_participant,
            Self::AssignedTo | Self::Participant => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn object() -> ScopedObject<'static> {
        ScopedObject {
            owned: true,
            collaborating: false,
            historical_read_participant: true,
            org_unit_id: Some("org"),
            settlement_party_id: Some("party"),
            warehouse_id: Some("warehouse"),
        }
    }

    #[test]
    fn empty_rules_never_grant_access() {
        assert!(!AccessRule::all(vec![]).allows_scope(&object()));
        assert!(!AccessRule::any(vec![]).allows_scope(&object()));
        assert!(!AccessRule::clause(&ScopeClause::default()).allows_scope(&object()));
        assert!(AccessRule::all(vec![AccessRule::Allow]).allows_scope(&object()));
    }

    #[test]
    fn warehouse_only_clause_does_not_require_ownership() {
        let clause =
            ScopeClause { warehouse_ids: BTreeSet::from(["warehouse".into()]), ..Default::default() };
        let rule = AccessRule::clause(&clause);
        let mut facts = object();
        facts.owned = false;
        facts.org_unit_id = None;
        assert!(rule.allows_scope(&facts));
        facts.warehouse_id = None;
        assert!(!rule.allows_scope(&facts));
    }

    #[test]
    fn ownership_union_is_intersected_with_other_dimensions() {
        let rule = AccessRule::clause(&ScopeClause {
            self_owned: true,
            org_unit_ids: BTreeSet::from(["org".into()]),
            settlement_party_ids: BTreeSet::from(["party".into()]),
            warehouse_ids: BTreeSet::from(["warehouse".into()]),
            ..Default::default()
        });
        let mut facts = object();
        facts.owned = false;
        assert!(rule.allows_scope(&facts));
        facts.org_unit_id = None;
        assert!(!rule.allows_scope(&facts));
        facts.owned = true;
        assert!(rule.allows_scope(&facts));
        facts.settlement_party_id = None;
        assert!(!rule.allows_scope(&facts));
    }

    #[test]
    fn history_cannot_bypass_user_limit_or_enable_writes() {
        let mut scope = ResolvedScope {
            role_clauses: vec![],
            user_limit: Some(ScopeClause {
                warehouse_ids: BTreeSet::from(["other".into()]),
                ..Default::default()
            }),
        };
        assert!(!AccessRule::scope(&scope, true).allows_scope(&object()));
        scope.user_limit = None;
        assert!(AccessRule::scope(&scope, true).allows_scope(&object()));
        assert!(!AccessRule::scope(&scope, false).allows_scope(&object()));
    }

    #[test]
    fn record_read_grants_extend_base_scope_but_not_independent_constraints() {
        let scope = ResolvedScope { role_clauses: vec![], user_limit: Some(ScopeClause::default()) };
        let read = AccessRule::any(vec![
            AccessRule::scope(&scope, false),
            AccessRule::RecordReadGrant(ResourceKind::SalesOrder),
        ]);
        let granted = |resource| resource == ResourceKind::SalesOrder;
        assert!(read.evaluate(&object(), &granted));
        assert!(!read.evaluate(&object(), &|_| false));
        assert!(!AccessRule::scope(&scope, false).evaluate(&object(), &granted));
        let protected = AccessRule::all(vec![read, AccessRule::Fact(AccessFact::Collaborating)]);
        assert!(!protected.evaluate(&object(), &granted));
    }
}
