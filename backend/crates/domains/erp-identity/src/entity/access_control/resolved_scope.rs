//! 范围分支与限制条件共用的对象判定，不承载角色资格解析。

use std::collections::BTreeSet;

use super::ScopeDimension;
use crate::entity::authorization::AccessRule;

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
    /// 当对象满足该角色的各适用维度时返回 `true`。
    /// `company` 为真时直接返回 `true`。否则已出现的归属、结算主体和仓库维度都要命中；没有任何正向范围时返回 `false`。
    ///
    /// `object`：业务域给出的对象责任事实。
    pub fn covers(&self, object: &ScopedObject<'_>) -> bool {
        AccessRule::clause(self).allows_scope(object)
    }

    // 检查单个角色条款是否覆盖资源所需维度。公司范围直接通过；其他范围不能借用别的角色补齐缺失维度。
    pub(crate) fn complete(&self, dimensions: &[ScopeDimension]) -> bool {
        self.company
            || dimensions.iter().all(|dimension| match dimension {
                ScopeDimension::InternalOrg => {
                    self.self_owned || self.collaborative || !self.org_unit_ids.is_empty()
                },
                ScopeDimension::SettlementParty => !self.settlement_party_ids.is_empty(),
                ScopeDimension::Warehouse => !self.warehouse_ids.is_empty(),
            })
    }
}

impl ResolvedScope {
    /// 判断读取或写入范围；调用方必须先证明有效账号和完整动作权限。
    /// 任一角色条款覆盖，或允许历史且对象是历史参与者时为真；若存在用户上限，还要再被上限覆盖。没有角色条款时，历史参与仍可单独放行读取。
    ///
    /// `object`：业务域给出的对象责任事实；`allow_history`：为真时，历史参与可以补读取。
    pub fn allows(&self, object: &ScopedObject<'_>, allow_history: bool) -> bool {
        AccessRule::scope(self, allow_history).allows_scope(object)
    }

    /// 当角色贡献该资源动作的正向范围时返回 `true`。
    /// 个人上限只收窄已有角色结果，不构成授权，也不得补 Company。
    ///
    /// 至少一条角色条款时为 `true`。
    pub fn has_role_scope(&self) -> bool {
        !self.role_clauses.is_empty()
    }
}
