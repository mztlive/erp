//! 将持久化范围字段解析为不可混用的内部目标形态。
use erp_core::{Error, Result};

use super::data_scope::normalize_scope_targets;
use super::{DataScopeType, ScopeBinding, ScopeDimension, ScopeTargetMode};

/// 已校验目标；各模式只携带适用字段，不改变持久化协议。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ScopeTarget {
    Company,
    SelfOwned,
    Collaborative,
    OrgExplicit { ids: Vec<String>, include_descendants: bool },
    OwnOrg { include_descendants: bool },
    SettlementParties(Vec<String>),
    Warehouses(Vec<String>),
}
/// 旧角色范围额外允许管理部门；人员表达式不承载该变体。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum BoundScopeTarget {
    Personal(ScopeTarget),
    ManagedOrgs,
}
impl BoundScopeTarget {
    /// 校验原始字段组合并规范化显式身份。
    /// # 参数
    /// binding、kind、targets 为持久化或请求中的原始字段。
    /// # 返回
    /// 已验证的内部目标形态。
    /// # 错误
    /// 非法模式组合、目标身份或数量拒绝。
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

    /// 从已通过模式组合检查的字段构造目标。
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

    /// 返回已验证范围的显式目标身份。
    /// # 参数
    /// 无。
    /// # 返回
    /// 静态目标切片，动态或隐式范围返回空切片。
    /// # 错误
    /// 不返回错误。
    pub(crate) fn ids(&self) -> &[String] {
        match self {
            Self::Personal(target) => target.ids(),
            Self::ManagedOrgs => &[],
        }
    }
}
impl ScopeTarget {
    /// 返回显式目标身份；动态与隐式范围不携带静态目标。
    /// # 参数
    /// 无。
    /// # 返回
    /// 已验证的静态目标切片。
    /// # 错误
    /// 不返回错误。
    pub(crate) fn ids(&self) -> &[String] {
        match self {
            Self::OrgExplicit { ids, .. } | Self::SettlementParties(ids) | Self::Warehouses(ids) => ids,
            _ => &[],
        }
    }
}
