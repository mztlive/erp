//! `data_scope`：数据范围（能看哪些客户、团队、组织和单据，数据模型 §5.1 / W19 §5.1）。

use std::collections::HashSet;

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::errors::{Error, Result};
use crate::ids::DataScopeId;
use crate::validation::normalize_required_text;

/// 主体 ID 最大长度。
const SUBJECT_ID_MAX_LEN: usize = 128;
/// 单个范围目标最大长度。
const TARGET_MAX_LEN: usize = 128;
/// 范围目标数量上限。
const MAX_TARGETS: usize = 128;

/// 范围主体类型（W19 §5.1：角色或用户）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash)]
#[serde(rename_all = "snake_case")]
pub enum DataScopeSubjectType {
    /// 角色。
    Role,
    /// 用户。
    User,
}

impl DataScopeSubjectType {
    /// 返回主体类型的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Role => "角色",
            Self::User => "用户",
        }
    }

    /// 返回主体类型的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Role => "role",
            Self::User => "user",
        }
    }
}

/// 范围类型（数据模型 §4.6 / W19 §5.1：公司、组织、团队、本人负责、协作等
/// 固定策略；固定枚举，其余范围类型属二期扩展的地基修订候选）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash)]
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
    /// # 返回
    /// 返回面向用户的中文标签。
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
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
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
    /// # 返回
    /// 组织/团队（需要目标对象）返回 `true`。
    pub fn requires_targets(self) -> bool {
        matches!(self, Self::Organization | Self::Team)
    }
}

/// 数据范围创建数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DataScopeData {
    /// 范围主体类型。
    pub subject_type: DataScopeSubjectType,
    /// 范围主体 ID（角色 ID 或用户 ID）。
    pub subject_id: String,
    /// 范围类型。
    pub scope_type: DataScopeType,
    /// 范围对象（组织/团队 ID；公司、本人负责、协作参与不携带目标）。
    pub scope_targets: Vec<String>,
}

/// 数据范围实体（数据模型 §5.1）。
///
/// 角色、用户、团队、权限和数据范围配置化（erp-phase-1 §11.1）；同一主体
/// 同一范围类型的唯一性由 P2 唯一索引与 P3 事务保证。客户历史参与者查看权
/// 来自 `document_participant`，不依赖当前数据范围反推（W19 §2.2）。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct DataScope {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 范围主体类型。
    pub subject_type: DataScopeSubjectType,
    /// 范围主体 ID。
    pub subject_id: String,
    /// 范围类型。
    pub scope_type: DataScopeType,
    /// 范围对象（去重保序）。
    pub scope_targets: Vec<String>,
}

impl DataScope {
    /// 创建数据范围。
    ///
    /// 完成 subject_id 的校验与规范化（trim、非空、长度上限），并强制范围
    /// 一致性：组织/团队必须携带范围目标（数量 ≤ `MAX_TARGETS`），公司/本人
    /// 负责/协作参与不允许携带目标；范围目标逐项 trim、去重、保序、长度上限。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::DataScopeId`）
    /// * `data` - 创建数据
    ///
    /// # 返回
    /// 返回新建的数据范围。
    ///
    /// # 错误
    /// 当主体 ID 为空/超长、目标数量越界、目标为空/超长，或范围类型与目标
    /// 携带不一致时返回错误。
    pub fn new(id: DataScopeId, data: DataScopeData) -> Result<Self> {
        let subject_id = normalize_required_text(
            data.subject_id,
            "范围主体ID不能为空",
            SUBJECT_ID_MAX_LEN,
            "范围主体ID过长",
        )?;
        let scope_targets = normalize_scope_targets(data.scope_type, data.scope_targets)?;
        Ok(Self {
            base: BaseModel::new(id.to_string()),
            subject_type: data.subject_type,
            subject_id,
            scope_type: data.scope_type,
            scope_targets,
        })
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
fn normalize_scope_targets(scope_type: DataScopeType, targets: Vec<String>) -> Result<Vec<String>> {
    if !scope_type.requires_targets() {
        if !targets.is_empty() {
            return Err(Error::from(format!(
                "{}范围不允许携带范围目标",
                scope_type.label()
            )));
        }
        return Ok(Vec::new());
    }
    if targets.is_empty() {
        return Err(Error::from(format!(
            "{}范围必须至少指定一个范围目标",
            scope_type.label()
        )));
    }
    if targets.len() > MAX_TARGETS {
        return Err(Error::from(format!("范围目标数量不能超过 {MAX_TARGETS}")));
    }
    let mut seen = HashSet::new();
    let mut normalized = Vec::with_capacity(targets.len());
    for target in targets {
        let target = normalize_required_text(target, "范围目标不能为空", TARGET_MAX_LEN, "范围目标过长")?;
        if seen.insert(target.clone()) {
            normalized.push(target);
        }
    }
    Ok(normalized)
}

#[cfg(test)]
mod tests {
    use super::{DataScope, DataScopeData, DataScopeSubjectType, DataScopeType};
    use crate::ids::DataScopeId;

    fn data() -> DataScopeData {
        DataScopeData {
            subject_type: DataScopeSubjectType::Role,
            subject_id: " role-sales ".to_string(),
            scope_type: DataScopeType::Team,
            scope_targets: vec![
                " team-1 ".to_string(),
                "team-2".to_string(),
                " team-1 ".to_string(),
            ],
        }
    }

    /// happy path：主体 ID trim，范围目标 trim + 去重保序。
    #[test]
    fn new_trims_subject_and_deduplicates_targets() {
        let scope = DataScope::new(DataScopeId::new("ds-1"), data()).unwrap();
        assert_eq!(scope.subject_id, "role-sales");
        assert_eq!(scope.scope_targets, vec!["team-1", "team-2"]);
        assert_eq!(scope.scope_type, DataScopeType::Team);
    }

    /// 失败路径：必填为空被拒。
    #[test]
    fn new_rejects_empty_subject_id() {
        let payload = DataScopeData {
            subject_id: "  ".to_string(),
            ..data()
        };
        assert!(DataScope::new(DataScopeId::new("ds-1"), payload).is_err());
    }

    /// 失败路径：列表越界（组织范围目标超上限）被拒。
    #[test]
    fn new_rejects_targets_over_capacity() {
        let payload = DataScopeData {
            scope_targets: (0..super::MAX_TARGETS + 1).map(|i| format!("team-{i}")).collect(),
            ..data()
        };
        assert!(DataScope::new(DataScopeId::new("ds-1"), payload).is_err());
    }

    /// 失败路径：关联不一致（范围类型与目标携带冲突）被拒。
    #[test]
    fn new_rejects_scope_type_target_mismatch() {
        let company_with_targets = DataScopeData {
            scope_type: DataScopeType::Company,
            scope_targets: vec!["team-1".to_string()],
            ..data()
        };
        assert!(DataScope::new(DataScopeId::new("ds-1"), company_with_targets).is_err());

        let team_without_targets = DataScopeData {
            scope_type: DataScopeType::Team,
            scope_targets: vec![],
            ..data()
        };
        assert!(DataScope::new(DataScopeId::new("ds-2"), team_without_targets).is_err());
    }

    /// 公司级与本人负责允许无目标。
    #[test]
    fn implicit_scopes_accept_empty_targets() {
        for scope_type in [
            DataScopeType::Company,
            DataScopeType::SelfOwned,
            DataScopeType::Collaborative,
        ] {
            let data = DataScopeData {
                scope_type,
                scope_targets: vec![],
                ..data()
            };
            let scope = DataScope::new(DataScopeId::new("ds-1"), data).unwrap();
            assert!(scope.scope_targets.is_empty());
        }
    }

    /// 枚举序列化与标签稳定。
    #[test]
    fn enums_codes_and_labels_are_stable() {
        assert_eq!(
            serde_json::to_string(&DataScopeType::SelfOwned).unwrap(),
            "\"self_owned\""
        );
        assert_eq!(
            serde_json::to_string(&DataScopeSubjectType::User).unwrap(),
            "\"user\""
        );
        assert_eq!(DataScopeType::Organization.as_str(), "organization");
        assert_eq!(DataScopeType::Team.label(), "团队");
        assert_eq!(DataScopeSubjectType::Role.label(), "角色");
    }

    /// BSON 往返。
    #[test]
    fn entity_roundtrips_through_bson() {
        let scope = DataScope::new(DataScopeId::new("ds-1"), data()).unwrap();
        let roundtrip: DataScope = bson::from_document(bson::to_document(&scope).unwrap()).unwrap();
        assert_eq!(roundtrip, scope);
    }
}
