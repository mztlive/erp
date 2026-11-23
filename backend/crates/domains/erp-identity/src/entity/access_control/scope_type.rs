//! 当前人员范围类型与显式目标规范化。
use std::collections::HashSet;

use erp_core::validation::normalize_required_text;
use erp_core::{Error, Result};
use serde::{Deserialize, Serialize};

/// 单个范围目标最大长度。
const TARGET_MAX_LEN: usize = 128;
/// 范围目标数量上限。
const MAX_TARGETS: usize = 128;

/// 范围目标的身份维度；各维度独立匹配，同名身份也不能混用。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum ScopeDimension {
    /// 匹配内部组织归属；静态与动态组织目标均使用此维度。
    InternalOrg,
    /// 匹配业务对象的结算主体，目标须显式指定。
    SettlementParty,
    /// 匹配业务对象的仓库，目标须显式指定。
    Warehouse,
}

/// 组织或团队范围取得目标身份的方式。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum ScopeTargetMode {
    /// 使用配置中明确列出的目标身份。
    Explicit,
    /// 按判断时点的有效成员关系取得本人的内部组织。
    OwnOrg,
}

/// 人员范围支持的固定类型，决定匹配依据以及是否需要解析目标。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum DataScopeType {
    /// 公司级范围，直接覆盖对象且无需目标。
    Company,
    /// 按绑定的维度和模式解析组织范围目标。
    Organization,
    /// 团队范围，沿用组织类的目标解析规则。
    Team,
    /// 按业务对象的本人负责事实限定范围。
    SelfOwned,
    /// 按业务对象的协作参与事实限定范围。
    Collaborative,
}

impl DataScopeType {
    /// 返回用于界面或校验提示的中文名称，依次为「公司级」「组织」「团队」「本人负责」「协作参与」。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Company => "公司级",
            Self::Organization => "组织",
            Self::Team => "团队",
            Self::SelfOwned => "本人负责",
            Self::Collaborative => "协作参与",
        }
    }

    /// 返回与存储协议一致的稳定代码：`company`、`organization`、`team`、`self_owned`、`collaborative`。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Company => "company",
            Self::Organization => "organization",
            Self::Team => "team",
            Self::SelfOwned => "self_owned",
            Self::Collaborative => "collaborative",
        }
    }

    /// 组织与团队进入目标解析，显式模式须提供目标列表；公司、本人负责和协作参与无需目标。
    pub fn requires_targets(self) -> bool {
        matches!(self, Self::Organization | Self::Team)
    }
}

/// 规范化显式目标：组织和团队要求去重前有 1 至 128 项，其他类型只接受空列表。
/// 每项去除首尾空白后须为 1 至 128 个字符，只允许 ASCII 字母、数字、下划线或短横线；去重时保留首次出现顺序。
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

    // 验证显式标识去除首尾空白并按首次出现顺序去重，数量及长度达到上限仍可接受。
    #[test]
    fn explicit_targets_are_normalized_with_stable_order() {
        let targets = vec![" team-1 ".into(), "team-2".into(), "team-1".into()];
        assert_eq!(normalize_scope_targets(DataScopeType::Team, targets).unwrap(), ["team-1", "team-2"]);
        let targets = (0..MAX_TARGETS).map(|index| format!("org-{index}")).collect::<Vec<_>>();
        assert_eq!(normalize_scope_targets(DataScopeType::Organization, targets.clone()).unwrap(), targets);
        let target = "a".repeat(TARGET_MAX_LEN);
        assert_eq!(normalize_scope_targets(DataScopeType::Team, vec![target.clone()]).unwrap(), [target]);
    }

    // 验证目标缺失、空白、通配符、显示名及数量或长度超限均被拒绝。
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

    // 验证隐式范围只接受空目标，序列化仍使用原有类型代码。
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
