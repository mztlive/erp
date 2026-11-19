//! 将持久化范围字段解析为不可混用的内部目标形态。
use erp_core::{Error, Result};

use super::scope_type::normalize_scope_targets;
use super::{DataScopeType, ScopeBinding, ScopeDimension, ScopeTargetMode};

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
        include_descendants: bool
    },
    /// 解析时以账号的有效成员关系确定目标组织。
    OwnOrg {
        /// 是否将本人组织的有效下级纳入范围。
        include_descendants: bool
    },
    /// 以对象结算主体匹配显式目标。
    SettlementParties(
        /// 经规范化的结算主体标识。
        Vec<String>
    ),
    /// 以对象仓库匹配显式目标。
    Warehouses(
        /// 经规范化的仓库标识。
        Vec<String>
    ),
}
/// 绑定解析结果，区分人员目标与旧角色专用的管理部门模式。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum BoundScopeTarget {
    /// 允许进入人员范围表达式的已验证目标。
    Personal(
        /// 按各维度和模式解释的目标内容。
        ScopeTarget
    ),
    /// 仅旧角色协议接受的动态管理部门标记。
    ManagedOrgs,
}
impl BoundScopeTarget {
    /// 将存储或请求中的目标字段解析成内部形态；无目标类型须省略目标设置，显式模式须提供目标。
    /// 动态模式只接受内部组织且目标须为空；下级标记仅对显式内部组织和本人组织必填。
    /// 显式目标会规范化，非法字段组合、身份或数量返回错误。
    pub(crate) fn parse(binding: &ScopeBinding, kind: DataScopeType, targets: &[String]) -> Result<Self> {
        if !kind.requires_targets() {
            if binding.target_mode.is_some() || binding.include_descendants.is_some() || !targets.is_empty() {
                return Err(Error::from("公司、本人及协作范围不得携带组织目标设置"));
            }
            return Ok(Self::Personal(match kind {
                DataScopeType::Company => ScopeTarget::Company,
                DataScopeType::SelfOwned => ScopeTarget::SelfOwned,
                DataScopeType::Collaborative => ScopeTarget::Collaborative,
                _ => unreachable!("需要目标的范围已排除"),
            }));
        }

        let mode = binding.target_mode.ok_or_else(|| Error::from("组织范围必须指定目标模式"))?;
        if mode != ScopeTargetMode::Explicit && binding.target_dimension != ScopeDimension::InternalOrg {
            return Err(Error::from("动态范围仅支持内部组织"));
        }
        if (mode == ScopeTargetMode::Explicit) == targets.is_empty() {
            return Err(Error::from("显式模式必须指定目标，动态模式不得混入静态目标"));
        }

        let needs_descendants =
            binding.target_dimension == ScopeDimension::InternalOrg && mode != ScopeTargetMode::ManagedOrgs;
        if needs_descendants != binding.include_descendants.is_some() {
            return Err(Error::from("内部组织显式或本人组织模式必须明确是否包含下级，其他模式不得设置"));
        }
        Self::from_shape(binding, kind, targets, mode)
    }

    // 字段组合校验通过后，按模式及维度构造目标；显式模式在此规范化标识。
    fn from_shape(
        binding: &ScopeBinding,
        kind: DataScopeType,
        targets: &[String],
        mode: ScopeTargetMode,
    ) -> Result<Self> {
        match mode {
            ScopeTargetMode::ManagedOrgs => Ok(Self::ManagedOrgs),
            ScopeTargetMode::OwnOrg => Ok(Self::Personal(ScopeTarget::OwnOrg {
                include_descendants: binding.include_descendants.expect("本人组织已要求下级标记"),
            })),
            ScopeTargetMode::Explicit => {
                let ids = normalize_scope_targets(kind, targets.to_vec())?;
                Ok(Self::Personal(match binding.target_dimension {
                    ScopeDimension::InternalOrg => ScopeTarget::OrgExplicit {
                        ids,
                        include_descendants: binding.include_descendants.expect("内部组织已要求下级标记"),
                    },
                    ScopeDimension::SettlementParty => ScopeTarget::SettlementParties(ids),
                    ScopeDimension::Warehouse => ScopeTarget::Warehouses(ids),
                }))
            },
        }
    }
}
impl ScopeTarget {
    /// 借用已规范化的显式目标标识；动态或隐式范围返回空切片。
    pub(crate) fn ids(&self) -> &[String] {
        match self {
            Self::OrgExplicit { ids, .. } | Self::SettlementParties(ids) | Self::Warehouses(ids) => ids,
            _ => &[],
        }
    }
}
