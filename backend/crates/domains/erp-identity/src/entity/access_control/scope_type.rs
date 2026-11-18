//! 当前人员范围类型与显式目标规范化。
use std::collections::HashSet;

use erp_core::validation::normalize_required_text;
use erp_core::{Error, Result};
use serde::{Deserialize, Serialize};

/// 单个范围目标最大长度。
const TARGET_MAX_LEN: usize = 128;
/// 范围目标数量上限。
const MAX_TARGETS: usize = 128;

/// 当前人员数据范围支持的固定策略类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum DataScopeType {
    /// 公司级。
    Company,
    /// 组织。
    Organization,
    /// 团队。
    Team,
    /// 本人负责。
    SelfOwned,
    /// 协作参与。
    Collaborative,
}

impl DataScopeType {
    /// 返回范围类型的中文展示名。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 依次为「公司级」「组织」「团队」「本人负责」「协作参与」。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Company => "公司级",
            Self::Organization => "组织",
            Self::Team => "团队",
            Self::SelfOwned => "本人负责",
            Self::Collaborative => "协作参与",
        }
    }

    /// 返回范围类型的稳定代码。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 依次为 `company`、`organization`、`team`、`self_owned`、`collaborative`。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Company => "company",
            Self::Organization => "organization",
            Self::Team => "team",
            Self::SelfOwned => "self_owned",
            Self::Collaborative => "collaborative",
        }
    }

    /// 判断范围类型是否需要显式范围目标。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// `Organization` 与 `Team` 返回 `true`。公司、本人负责和协作参与返回 `false`。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn requires_targets(self) -> bool {
        matches!(self, Self::Organization | Self::Team)
    }
}

/// 规范化范围目标列表：按范围类型校验携带一致性、数量上限与逐项 trim/去重。
///
/// # 参数
/// * `scope_type` - 范围类型
/// * `targets` - 原始目标列表
///
/// # 返回
/// 返回去重保序后的目标列表。
///
/// # 错误
/// 当目标数量越界、目标为空/超长或范围类型不允许携带目标时返回错误。
pub(crate) fn normalize_scope_targets(
    scope_type: DataScopeType,
    targets: Vec<String>,
) -> Result<Vec<String>> {
    if !scope_type.requires_targets() {
        if !targets.is_empty() {
            return Err(Error::from(format!("{}范围不允许携带范围目标", scope_type.label())));
        }
        return Ok(Vec::new());
    }
    if targets.is_empty() {
        return Err(Error::from(format!("{}范围必须至少指定一个范围目标", scope_type.label())));
    }
    if targets.len() > MAX_TARGETS {
        return Err(Error::from(format!("范围目标数量不能超过 {MAX_TARGETS}")));
    }

    let mut seen = HashSet::new();
    let mut normalized = Vec::with_capacity(targets.len());
    for target in targets {
        let target = normalize_required_text(target, "范围目标不能为空", TARGET_MAX_LEN, "范围目标过长")?;
        if !target.bytes().all(|byte| byte.is_ascii_alphanumeric() || b"_-".contains(&byte)) {
            return Err(Error::from("范围目标必须为稳定身份，不允许通配符或显示名称"));
        }
        if seen.insert(target.clone()) {
            normalized.push(target);
        }
    }
    Ok(normalized)
}

#[cfg(test)]
mod tests {
    use super::{DataScopeType, MAX_TARGETS, TARGET_MAX_LEN, normalize_scope_targets};

    /// 显式身份去空白、去重保序，同时接受数量和长度边界。
    #[test]
    fn explicit_targets_are_normalized_with_stable_order() {
        let targets = vec![" team-1 ".into(), "team-2".into(), "team-1".into()];
        assert_eq!(normalize_scope_targets(DataScopeType::Team, targets).unwrap(), ["team-1", "team-2"]);
        let targets = (0..MAX_TARGETS).map(|index| format!("org-{index}")).collect::<Vec<_>>();
        assert_eq!(normalize_scope_targets(DataScopeType::Organization, targets.clone()).unwrap(), targets);
        let target = "a".repeat(TARGET_MAX_LEN);
        assert_eq!(normalize_scope_targets(DataScopeType::Team, vec![target.clone()]).unwrap(), [target]);
    }

    /// 目标形态、数量、身份内容和长度非法时不得生成可用范围。
    #[test]
    fn explicit_targets_reject_invalid_identity_and_capacity() {
        for targets in [
            vec![],
            vec![" ".into()],
            vec!["*".into()],
            vec!["部门名称".into()],
            vec!["a".repeat(TARGET_MAX_LEN + 1)],
            (0..=MAX_TARGETS).map(|index| format!("org-{index}")).collect(),
        ] {
            assert!(normalize_scope_targets(DataScopeType::Organization, targets).is_err());
        }
    }

    /// 隐式范围不接受静态目标，范围代码保持现有序列化协议。
    #[test]
    fn implicit_scopes_reject_targets_and_keep_wire_codes() {
        for (kind, code) in [
            (DataScopeType::Company, "company"),
            (DataScopeType::SelfOwned, "self_owned"),
            (DataScopeType::Collaborative, "collaborative"),
        ] {
            assert!(normalize_scope_targets(kind, vec![]).unwrap().is_empty());
            assert!(normalize_scope_targets(kind, vec!["org-1".into()]).is_err());
            assert_eq!(serde_json::to_value(kind).unwrap(), code);
        }
    }
}
