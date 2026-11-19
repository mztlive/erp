use std::collections::BTreeSet;

use super::{FactKind, RelationKey, RelationMatch};
use crate::Permission;
use crate::entity::access_control::{ResolvedScope, ScopeClause, ScopedObject};
use crate::entity::resource_access::ResourceKind;

/// 由对象取证或查询映射解释的单项业务条件；涉及账号时以本次上下文为准。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AccessFact {
    /// 当前账号是对象的负责人之一。
    Owned,
    /// 当前账号在对象的协作名单中。
    Collaborating,
    /// 对象至少一个归属组织落在指定范围内。
    OrganizationIn(
        /// 范围条款允许的组织 ID 集合。
        BTreeSet<String>,
    ),
    /// 对象的结算主体落在指定范围内。
    SettlementPartyIn(
        /// 范围条款允许的结算主体 ID 集合。
        BTreeSet<String>,
    ),
    /// 对象的所属仓库落在指定范围内。
    WarehouseIn(
        /// 范围条款允许的仓库 ID 集合。
        BTreeSet<String>,
    ),
    /// 当前账号是历史参与人，供允许历史读取的规则使用。
    HistoryParticipant,
    /// 当前账号是任务的现任处理人。
    AssignedTo,
    /// 当前账号属于任务参与人集合。
    Participant,
    /// 比较任务实际完成标记，由身份域与指派、参与条件组合。
    Completed(
        /// 规则要求的完成标记取值。
        bool,
    ),
}

/// 身份域内部构造的条件树，供对象判断和查询编译共用。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum AccessRule {
    /// 当前节点无需附加数据限制。
    Allow,
    /// 当前节点拒绝访问。
    Deny,
    /// 要求所有分支满足，空交集拒绝访问。
    All(
        /// 必须共同成立的条件分支。
        Vec<Self>,
    ),
    /// 至少一个分支满足，空并集拒绝访问。
    Any(
        /// 可分别满足此节点的条件分支。
        Vec<Self>,
    ),
    /// 匹配当前对象的一项事实条件。
    Fact(
        /// 对象取证与查询映射共用的条件。
        AccessFact,
    ),
    /// 求值前先核验对象事实完整性。
    Required {
        /// 内层条件执行前必须装载的事实维度。
        facts: BTreeSet<FactKind>,
        /// 完整性检查通过后执行的条件。
        rule: Box<Self>,
    },
    /// 按独立入口的权限上下文解释内层数据条件。
    Independent {
        /// 单独取证并绑定到查询版本的功能权限。
        permission: Permission,
        /// 以授予该权限的有效角色计算的数据条件。
        rule: Box<Self>,
    },
    /// 以有效的指定记录授权补充读取范围。
    RecordReadGrant(
        /// 记录授权须匹配的资源类型。
        ResourceKind,
    ),
    /// 检查已核验的来源关系是否有实际引用。
    RelationPresence {
        /// 待读取或映射的来源关系登记键。
        relation: RelationKey,
        /// `true` 要求有实际引用，`false` 要求引用为空。
        present: bool,
    },
    /// 对真实来源执行内层条件，再判断来源覆盖。
    Related {
        /// 保存完整来源引用的关系登记键。
        relation: RelationKey,
        /// 来源对象必须匹配的业务资源。
        resource: String,
        /// 判断有权来源是否足以覆盖实际引用的方式。
        mode: RelationMatch,
        /// 对每个来源对象分别执行的条件。
        rule: Box<Self>,
    },
}

impl AccessRule {
    /// 归一化交集；空输入拒绝，显式放行节点可约去，已有取证要求仍须保留。
    pub(crate) fn all(rules: Vec<Self>) -> Self {
        if rules.is_empty() || (rules.contains(&Self::Deny) && !rules.iter().any(Self::has_requirements)) {
            return Self::Deny;
        }
        let mut rules: Vec<_> = rules.into_iter().filter(|rule| *rule != Self::Allow).collect();
        match rules.len() {
            0 => Self::Allow,
            1 => rules.pop().expect("已验证只有一条规则"),
            _ => Self::All(rules),
        }
    }

    /// 归一化并集；空输入拒绝，显式拒绝节点可约去，无条件放行仍保留所需取证。
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

    // 识别仍需取证的分支，避免归一化约掉事实、来源或完整性检查。
    fn has_requirements(&self) -> bool {
        match self {
            Self::Required { facts, rule } => !facts.is_empty() || rule.has_requirements(),
            Self::All(items) | Self::Any(items) => items.iter().any(Self::has_requirements),
            Self::Fact(_) | Self::Related { .. } | Self::RelationPresence { .. } => true,
            Self::Independent { rule, .. } => rule.has_requirements(),
            _ => false,
        }
    }

    /// 为规则加上求值前必查的事实维度；要求为空时原样返回。
    pub(crate) fn requiring(self, facts: &BTreeSet<FactKind>) -> Self {
        if facts.is_empty() { self } else { Self::Required { facts: facts.clone(), rule: Box::new(self) } }
    }

    /// 合并角色条款与获准的历史参与，再与个人上限求交；公司范围且无上限时可直接放行。
    pub(crate) fn scope(scope: &ResolvedScope, allow_history: bool) -> Self {
        let granted = if scope.user_limit.is_none() && scope.role_clauses.iter().any(|clause| clause.company)
        {
            Self::Allow
        } else {
            let mut grants: Vec<_> = scope.role_clauses.iter().map(Self::clause).collect();
            if allow_history {
                grants.push(Self::Fact(AccessFact::HistoryParticipant));
            }
            Self::any(grants)
        };

        match &scope.user_limit {
            Some(limit) => Self::all(vec![granted, Self::clause(limit)]),
            None => granted,
        }
    }

    /// 公司条款放行；其余条款将本人、协作和组织取并集，再与已配置的结算主体及仓库条件求交。
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

    /// 用调用方准备的范围事实求值；记录补权和完整对象取证只在对象授权中执行。
    pub(crate) fn allows_scope(&self, object: &ScopedObject<'_>) -> bool {
        match self {
            Self::Allow => true,
            Self::All(rules) => !rules.is_empty() && rules.iter().all(|rule| rule.allows_scope(object)),
            Self::Any(rules) => rules.iter().any(|rule| rule.allows_scope(object)),
            Self::Fact(fact) => fact.matches_scope(object),
            Self::Independent { rule, .. } => rule.allows_scope(object),
            Self::Deny
            | Self::RecordReadGrant(_)
            | Self::Related { .. }
            | Self::RelationPresence { .. }
            | Self::Required { .. } => false,
        }
    }
}

impl AccessFact {
    // 匹配单个范围维度；任务指派、参与和完成条件需使用完整对象事实。
    fn matches_scope(&self, object: &ScopedObject<'_>) -> bool {
        match self {
            Self::Owned => object.owned,
            Self::Collaborating => object.collaborating,
            Self::OrganizationIn(ids) => object.org_unit_id.is_some_and(|id| ids.contains(id)),
            Self::SettlementPartyIn(ids) => object.settlement_party_id.is_some_and(|id| ids.contains(id)),
            Self::WarehouseIn(ids) => object.warehouse_id.is_some_and(|id| ids.contains(id)),
            Self::HistoryParticipant => object.historical_read_participant,
            Self::AssignedTo | Self::Participant | Self::Completed(_) => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::{FactValue, ObjectFacts};
    use super::*;
    use crate::Error;

    // 构造有本人归属、历史参与及三个范围维度的测试对象。
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

    // 验证空交集、空并集及空条款均拒绝，只有显式放行才允许无限制访问。
    #[test]
    fn empty_rules_never_grant_access() {
        assert!(!AccessRule::all(vec![]).allows_scope(&object()));
        assert!(!AccessRule::any(vec![]).allows_scope(&object()));
        assert!(!AccessRule::clause(&ScopeClause::default()).allows_scope(&object()));
        assert!(AccessRule::all(vec![AccessRule::Allow]).allows_scope(&object()));
    }

    // 验证只配置仓库的条款独立生效，仓库缺失时拒绝。
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

    // 验证本人或组织命中可满足归属维度，其他已配置维度仍须同时命中。
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

    // 验证历史参与只在启用时补充范围，且仍受个人上限约束。
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

    // 验证公司范围只在无个人上限时跳过历史取证，设置上限后仍须核验本人条件。
    #[test]
    fn company_scope_skips_history_only_without_a_user_limit() {
        let mut scope = ResolvedScope {
            role_clauses: vec![ScopeClause { company: true, ..Default::default() }],
            user_limit: None,
        };
        let grants = BTreeSet::new();
        let current = ObjectFacts::default();
        assert!(AccessRule::scope(&scope, true).allows_object("user", &current, &grants).unwrap());

        scope.user_limit = Some(ScopeClause { self_owned: true, ..Default::default() });
        let mut current = current;
        current.owner_id = FactValue::Known(Some("other".into()));
        assert!(matches!(
            AccessRule::scope(&scope, true).allows_object("user", &current, &grants),
            Err(Error::ValidationError(_))
        ));
        current.history_participants = FactValue::Known(["user".into()].into());
        assert!(!AccessRule::scope(&scope, true).allows_object("user", &current, &grants).unwrap());
        current.owner_id = FactValue::Known(Some("user".into()));
        assert!(AccessRule::scope(&scope, true).allows_object("user", &current, &grants).unwrap());
    }

    // 验证记录读取授权仅扩充基础规则的读取分支，独立条件仍可阻止访问。
    #[test]
    fn record_read_grants_extend_base_scope_but_not_independent_constraints() {
        let scope = ResolvedScope { role_clauses: vec![], user_limit: Some(ScopeClause::default()) };
        let read = AccessRule::any(vec![
            AccessRule::scope(&scope, false),
            AccessRule::RecordReadGrant(ResourceKind::SalesOrder),
        ]);
        let record = ObjectFacts {
            resource: "sales_order".into(),
            id: "sale".into(),
            collaborators: FactValue::Known(BTreeSet::new()),
            ..Default::default()
        };
        let grants = [(ResourceKind::SalesOrder, record.id.clone())].into();
        assert!(read.allows_object("user", &record, &grants).unwrap());
        assert!(!read.allows_object("user", &record, &BTreeSet::new()).unwrap());
        assert!(!read.allows_scope(&object()));
        assert!(!AccessRule::scope(&scope, false).allows_object("user", &record, &grants).unwrap());

        let protected = AccessRule::all(vec![read, AccessRule::Fact(AccessFact::Collaborating)]);
        assert!(!protected.allows_object("user", &record, &grants).unwrap());
    }
}
