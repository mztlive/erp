//! 将无角色的人员范围项直接编译为查询和对象判定共用的条款。
use std::collections::BTreeSet;

use erp_core::common::time::Instant;

use super::PersonScopeTerm;
use crate::access_control::{DataScopeType, ScopeBinding, ScopeClause, ScopeDimension, ScopeTargetMode};
use crate::entity::access_control::data_scope::normalize_scope_targets;
use crate::entity::organization::{OrgMembership, OrgTree};
use crate::{Error, Result};

pub(super) struct ScopeCompiler<'a, 'tree> {
    pub user: &'a str,
    pub resource: &'a str,
    pub action: &'a str,
    pub tree: &'a OrgTree<'tree>,
    pub memberships: &'a [OrgMembership],
    pub at: Instant,
}

impl ScopeCompiler<'_, '_> {
    /// 编译一个完整分支；同维度合并，维度完整性由调用方检查。
    /// # 参数
    /// terms 为同一分支的完整范围项。
    /// # 返回
    /// 可用于数据库查询和对象检查的条款。
    /// # 错误
    /// 目标形态非法、旧角色动态模式或组织不存在时拒绝。
    pub(super) fn clause(&self, terms: &[PersonScopeTerm]) -> Result<ScopeClause> {
        let mut clause = ScopeClause::default();
        for term in terms {
            let targets = term.targets(self.resource, self.action)?;
            match term.scope_type {
                DataScopeType::Company => clause.company = true,
                DataScopeType::SelfOwned => clause.self_owned = true,
                DataScopeType::Collaborative => clause.collaborative = true,
                DataScopeType::Organization | DataScopeType::Team => match term.target_dimension {
                    ScopeDimension::InternalOrg => {
                        clause.org_unit_ids.extend(self.organizations(term, &targets)?)
                    },
                    ScopeDimension::SettlementParty => clause.settlement_party_ids.extend(targets),
                    ScopeDimension::Warehouse => clause.warehouse_ids.extend(targets),
                },
            }
        }
        Ok(clause)
    }

    /// 解析当前有效本人组织或显式组织，不读取旧角色关系。
    fn organizations(&self, term: &PersonScopeTerm, targets: &[String]) -> Result<BTreeSet<String>> {
        let ids: Vec<&str> = match term.target_mode {
            Some(ScopeTargetMode::Explicit) => targets.iter().map(String::as_str).collect(),
            Some(ScopeTargetMode::OwnOrg) => self
                .memberships
                .iter()
                .filter(|m| !m.base.is_deleted() && m.user_id == self.user && m.validity.contains(self.at))
                .map(|m| m.org_unit_id.as_str())
                .collect(),
            _ => unreachable!("范围项已验证动态模式"),
        };
        let mut result = BTreeSet::new();
        for id in ids {
            result.extend(self.tree.expand(id, term.include_descendants.unwrap_or(false))?);
        }
        Ok(result)
    }
}

impl PersonScopeTerm {
    /// 在表达式编译边界校验形态并规范化目标，与旧数据迁移使用相同规则。
    fn targets(&self, resource: &str, action: &str) -> Result<Vec<String>> {
        if self.target_mode == Some(ScopeTargetMode::ManagedOrgs) {
            return Err(Error::ValidationError("人员范围不接受角色管理部门模式".into()));
        }
        let binding = ScopeBinding {
            schema_version: 2,
            resource: resource.into(),
            actions: vec![action.into()],
            target_dimension: self.target_dimension,
            target_mode: self.target_mode,
            include_descendants: self.include_descendants,
            enabled: true,
        };
        binding.validate(self.scope_type, &self.scope_targets)?;
        if self.target_mode == Some(ScopeTargetMode::Explicit) || !self.scope_type.requires_targets() {
            Ok(normalize_scope_targets(self.scope_type, self.scope_targets.clone())?)
        } else {
            Ok(Vec::new())
        }
    }
}
