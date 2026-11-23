//! 将持久化范围字段解析为不可混用的内部目标形态。
use erp_core::Error as CoreError;

use super::person_scope::PersonScopeTerm;
use super::scope_type::normalize_scope_targets;
use super::{DataScopeType, ScopeDimension, ScopeTargetMode};
use crate::Result;

/// 已校验的人员范围目标，各模式只携带适用信息，沿用原有持久化协议。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ScopeTarget {
    /// 公司级范围，无需具体目标身份。
    Company,
    /// 依据对象的本人负责事实匹配。
    SelfOwned,
    /// 依据对象的协作参与事实匹配。
    Collaborative,
    /// 使用显式内部组织目标，解析时按标记展开组织树。
    OrgExplicit {
        /// 经规范化的内部组织标识，供组织树展开。
        ids: Vec<String>,
        /// 是否将显式组织目标的有效下级纳入范围。
        include_descendants: bool,
    },
    /// 解析时以账号的有效成员关系确定目标组织。
    OwnOrg {
        /// 是否将本人组织的有效下级纳入范围。
        include_descendants: bool,
    },
    /// 以对象结算主体匹配显式目标。
    SettlementParties(
        /// 经规范化的结算主体标识。
        Vec<String>,
    ),
    /// 以对象仓库匹配显式目标。
    Warehouses(
        /// 经规范化的仓库标识。
        Vec<String>,
    ),
}

impl ScopeTarget {
    /// 校验人员范围项的目标字段并规范化显式标识；角色管理部门模式不会产生人员目标。
    pub(crate) fn parse(term: &PersonScopeTerm) -> Result<Self> {
        Self::parse_with_ids(term, term.scope_targets.clone())
    }

    /// 消费待规范化的显式 ID，供拥有输入的保存路径避免复制。
    pub(crate) fn parse_with_ids(term: &PersonScopeTerm, ids: Vec<String>) -> Result<Self> {
        if !term.scope_type.requires_targets() {
            if term.target_mode.is_some() || term.include_descendants.is_some() || !ids.is_empty() {
                return Err(CoreError::from("公司、本人及协作范围不得携带组织目标设置").into());
            }
            return Ok(match term.scope_type {
                DataScopeType::Company => Self::Company,
                DataScopeType::SelfOwned => Self::SelfOwned,
                DataScopeType::Collaborative => Self::Collaborative,
                _ => unreachable!("需要目标的范围已排除"),
            });
        }

        let mode = term.target_mode.ok_or_else(|| CoreError::from("组织范围必须指定目标模式"))?;
        if mode != ScopeTargetMode::Explicit && term.target_dimension != ScopeDimension::InternalOrg {
            return Err(CoreError::from("动态范围仅支持内部组织").into());
        }
        if (mode == ScopeTargetMode::Explicit) == ids.is_empty() {
            return Err(CoreError::from("显式模式必须指定目标，动态模式不得混入静态目标").into());
        }
        let needs_descendants = term.target_dimension == ScopeDimension::InternalOrg;
        if needs_descendants != term.include_descendants.is_some() {
            return Err(
                CoreError::from("内部组织显式或本人组织模式必须明确是否包含下级，其他模式不得设置").into()
            );
        }

        match mode {
            ScopeTargetMode::OwnOrg => Ok(Self::OwnOrg {
                include_descendants: term.include_descendants.expect("本人组织已要求下级标记"),
            }),
            ScopeTargetMode::Explicit => {
                let ids = normalize_scope_targets(term.scope_type, ids)?;
                Ok(match term.target_dimension {
                    ScopeDimension::InternalOrg => Self::OrgExplicit {
                        ids,
                        include_descendants: term.include_descendants.expect("内部组织已要求下级标记"),
                    },
                    ScopeDimension::SettlementParty => Self::SettlementParties(ids),
                    ScopeDimension::Warehouse => Self::Warehouses(ids),
                })
            },
        }
    }

    /// 事务只校验显式目标的存在性，动态和隐式范围由运行时解释。
    pub(crate) fn explicit(&self) -> Option<(ScopeDimension, &[String])> {
        match self {
            Self::OrgExplicit { ids, .. } => Some((ScopeDimension::InternalOrg, ids)),
            Self::SettlementParties(ids) => Some((ScopeDimension::SettlementParty, ids)),
            Self::Warehouses(ids) => Some((ScopeDimension::Warehouse, ids)),
            _ => None,
        }
    }

    /// 移出显式目标 ID，供规范化后的条款继续持有。
    pub(crate) fn into_ids(self) -> Vec<String> {
        match self {
            Self::OrgExplicit { ids, .. } | Self::SettlementParties(ids) | Self::Warehouses(ids) => ids,
            _ => Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Error;

    // 多个字段不合法时，沿用先维度、再静态目标、最后下级标记的报错顺序。
    #[test]
    fn malformed_dynamic_mode_reports_the_first_shape_error() {
        let mut term = PersonScopeTerm {
            scope_type: DataScopeType::Team,
            target_dimension: ScopeDimension::Warehouse,
            target_mode: Some(ScopeTargetMode::OwnOrg),
            include_descendants: None,
            scope_targets: vec!["one".into()],
        };
        let assert_error = |term: &PersonScopeTerm, expected: &str| {
            assert!(matches!(
                ScopeTarget::parse(term),
                Err(Error::Logic(CoreError::LogicError(message))) if message == expected
            ));
        };
        assert_error(&term, "动态范围仅支持内部组织");
        term.target_dimension = ScopeDimension::InternalOrg;
        assert_error(&term, "显式模式必须指定目标，动态模式不得混入静态目标");
        term.scope_targets.clear();
        assert_error(&term, "内部组织显式或本人组织模式必须明确是否包含下级，其他模式不得设置");
    }
}
