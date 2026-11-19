//! 将无角色的人员范围项直接编译为查询和对象判定共用的条款。
use std::collections::BTreeSet;

use erp_core::common::time::Instant;

use super::PersonScopeTerm;
use crate::Result;
use crate::access_control::ScopeClause;
use crate::entity::access_control::scope_target::ScopeTarget;
use crate::entity::organization::{OrgMembership, OrgTree};

/// 单次人员范围解析的编译上下文，让各分支与上限复用同一份组织事实和判断时点。
pub(super) struct ScopeCompiler<'a, 'tree> {
    /// 人员配置所属账号，用于筛选本人组织成员关系。
    pub user: &'a str,
    /// 范围所属资源，用于校验各项的目标绑定。
    pub resource: &'a str,
    /// 本次解析的动作，各项按此动作绑定。
    pub action: &'a str,
    /// 经校验的组织树，用于展开显式组织目标和本人组织。
    pub tree: &'a OrgTree<'tree>,
    /// 本次读取的成员关系，用于解析本人组织模式。
    pub memberships: &'a [OrgMembership],
    /// 本次判断时点，用于筛选有效期内的成员关系。
    pub at: Instant,
}

impl ScopeCompiler<'_, '_> {
    /// 将同组范围项合成查询和对象检查共用的条款，同维度目标取并集，必需维度由调用方检查。
    /// 本人组织来自判断时点有效且未删除的成员关系；非法目标、旧角色管理部门模式或未知组织返回错误。
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

    // 展开显式目标或有效成员关系中的组织，按下级标记合并有效节点并跳过停用分支；未知组织报错。
    fn organizations(&self, ids: &[String], include_descendants: bool) -> Result<BTreeSet<String>> {
        let mut result = BTreeSet::new();
        for id in ids {
            result.extend(self.tree.expand(id, include_descendants)?);
        }
        Ok(result)
    }
}
