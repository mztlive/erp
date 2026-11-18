//! 将无角色的人员范围项直接编译为查询和对象判定共用的条款。
use std::collections::BTreeSet;

use erp_core::common::time::Instant;

use super::PersonScopeTerm;
use crate::Result;
use crate::access_control::ScopeClause;
use crate::entity::access_control::scope_target::ScopeTarget;
use crate::entity::organization::{OrgMembership, OrgTree};

pub(super) struct ScopeCompiler<'a, 'tree> {
    pub user: &'a str,
    pub resource: &'a str,
    pub action: &'a str,
    pub tree: &'a OrgTree<'tree>,
    pub memberships: &'a [OrgMembership],
    pub at: Instant,
}

impl ScopeCompiler<'_, '_> {
    // 编译一个完整分支；同维度合并，维度完整性由调用方检查。
    // 可用于数据库查询和对象检查的条款。
    //
    // 目标形态非法、旧角色动态模式或组织不存在时拒绝。
    //
    // terms 为同一分支的完整范围项。
    pub(super) fn clause(&self, terms: &[PersonScopeTerm]) -> Result<ScopeClause> {
        let mut clause = ScopeClause::default();
        for term in terms {
            match term.target(self.resource, &[self.action.into()])? {
                ScopeTarget::Company => clause.company = true,
                ScopeTarget::SelfOwned => clause.self_owned = true,
                ScopeTarget::Collaborative => clause.collaborative = true,
                ScopeTarget::OrgExplicit { ids, include_descendants } => {
                    clause.org_unit_ids.extend(self.organizations(&ids, include_descendants)?)
                },
                ScopeTarget::OwnOrg { include_descendants } => {
                    let ids = self
                        .memberships
                        .iter()
                        .filter(|m| {
                            !m.base.is_deleted() && m.user_id == self.user && m.validity.contains(self.at)
                        })
                        .map(|m| m.org_unit_id.clone())
                        .collect::<Vec<_>>();
                    clause.org_unit_ids.extend(self.organizations(&ids, include_descendants)?);
                },
                ScopeTarget::SettlementParties(ids) => clause.settlement_party_ids.extend(ids),
                ScopeTarget::Warehouses(ids) => clause.warehouse_ids.extend(ids),
            }
        }
        Ok(clause)
    }

    // 授权编译需要组织维度时，按当前解释时点解析本人主属或显式部门，不读取旧角色关系。
    fn organizations(&self, ids: &[String], include_descendants: bool) -> Result<BTreeSet<String>> {
        let mut result = BTreeSet::new();
        for id in ids {
            result.extend(self.tree.expand(id, include_descendants)?);
        }
        Ok(result)
    }
}
