//! 人员资料的原子变更：限定单人，按差异更新组织关系。
use std::collections::BTreeSet;

use erp_core::common::time::Instant;
use serde::{Deserialize, Serialize};

use super::organization_change::OrganizationOperation;
use crate::{Error, Result, RoleId, RoleIdSet};

/// 旧人员资料请求中的管理关系明细，仅供历史审计载荷往返。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct PersonManagementChange {
    /// 此次历史授予绑定的角色 ID。
    pub role_id: String,
    /// 此次历史授予所覆盖的起始组织 ID。
    pub org_unit_id: String,
    /// 当时是否同时授予下级组织的管理范围。
    pub include_descendants: bool,
    /// 当时指定的失效时点，`None` 表示无固定终点。
    pub valid_to: Option<Instant>,
}

/// 对单个账号的姓名、完整角色集和主属组织按差异执行原子修改。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PersonProfileChange {
    /// 要修改人员资料的账号 ID。
    pub user_id: String,
    /// 编辑时读到的原姓名，服务在修改姓名前检查是否仍一致。
    pub expected_name: String,
    /// 拟保存的新姓名，`None` 表示保持原姓名。
    pub name: Option<String>,
    /// 编辑时的完整角色集，替换角色时必填，用于识别并发变更。
    pub expected_role_ids: Option<RoleIdSet>,
    /// 要替换成的完整非空角色集，`None` 表示保留现有角色。
    pub role_ids: Option<RoleIdSet>,
    /// 要调入的主属组织 ID，`None` 表示保持原组织关系。
    pub org_unit_id: Option<String>,
    /// 历史请求要移除的管理关系 ID；当前写入要求为空，空值不序列化。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) remove_management_ids: Vec<String>,
    /// 历史请求要新增的管理关系；当前写入要求为空，空值不序列化。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) add_management: Vec<PersonManagementChange>,
}

impl PersonProfileChange {
    /// 替换角色时要求新集合非空，并将期望集合与当前授权快照 `current` 按集合比较，忽略顺序。
    /// 缺少期望集合或集合已变化时拒绝；未提供新角色集时直接通过，旧管理关系不参与校验。
    pub fn validate_roles(&self, current: &[RoleId]) -> Result<()> {
        let Some(roles) = &self.role_ids else {
            return Ok(());
        };
        roles.ensure_non_empty()?;
        let expected = self
            .expected_role_ids
            .as_ref()
            .ok_or_else(|| Error::ValidationError("修改角色必须提供原角色集合".into()))?;
        if expected.iter().collect::<BTreeSet<_>>() != current.iter().collect::<BTreeSet<_>>() {
            return Err(Error::ConflictError("人员角色已变化，请刷新后重新编辑".into()));
        }
        Ok(())
    }

    /// 将目标人员的主属组织修改展开为一条调岗命令；只改姓名或角色时返回空列表。
    /// 人员 ID 为空白、没有修改内容或试图修改旧管理关系时拒绝。
    pub fn operations(&self) -> Result<Vec<OrganizationOperation>> {
        if self.user_id.trim().is_empty() {
            return Err(Error::ValidationError("人员不能为空".into()));
        }
        if !self.remove_management_ids.is_empty() || !self.add_management.is_empty() {
            return Err(Error::ValidationError("部门管理关系已停用，请在人员数据范围中设置授权".into()));
        }

        let mut operations = Vec::new();
        if let Some(org_unit_id) = &self.org_unit_id {
            operations.push(OrganizationOperation::TransferMember {
                user_id: self.user_id.clone(),
                org_unit_id: org_unit_id.clone(),
            });
        }
        if operations.is_empty() && self.name.is_none() && self.role_ids.is_none() {
            return Err(Error::ValidationError("没有需要保存的修改".into()));
        }
        Ok(operations)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // 构造有原姓名但尚未填写改动的单人资料请求，供各测试增补差异。
    fn change() -> PersonProfileChange {
        PersonProfileChange {
            user_id: "one".into(),
            expected_name: "旧姓名".into(),
            name: None,
            expected_role_ids: None,
            role_ids: None,
            org_unit_id: None,
            remove_management_ids: vec![],
            add_management: vec![],
        }
    }
    // 验证仅改角色不生成调岗，期望集合忽略顺序，缺失或过期快照及空新集合均拒绝。
    #[test]
    fn role_change_checks_original_set_and_accepts_role_only_change() {
        let mut value = change();
        value.role_ids = Some(RoleIdSet::parse(["new"]).unwrap());
        let current = vec![RoleId::parse("old").unwrap()];
        assert!(value.validate_roles(&current).is_err());
        value.expected_role_ids = Some(RoleIdSet::parse(["other"]).unwrap());
        assert!(matches!(value.validate_roles(&current), Err(Error::ConflictError(_))));
        value.expected_role_ids = Some(RoleIdSet::parse(&current).unwrap());
        assert!(value.validate_roles(&current).is_ok());
        assert!(value.operations().unwrap().is_empty());
        value.expected_role_ids = Some(RoleIdSet::parse(["second", "old"]).unwrap());
        assert!(
            value.validate_roles(&[RoleId::parse("old").unwrap(), RoleId::parse("second").unwrap()]).is_ok()
        );
        value.expected_role_ids = Some(RoleIdSet::parse(&current).unwrap());
        value.role_ids = Some(RoleIdSet::default());
        assert!(value.validate_roles(&current).is_err());
    }

    // 验证主属组织变更只生成一条针对目标人员的调岗命令。
    #[test]
    fn profile_expands_only_target_person() {
        let mut value = change();
        value.org_unit_id = Some("sales".into());
        let ops = value.operations().unwrap();
        assert_eq!(ops.len(), 1);
        assert!(matches!(&ops[0], OrganizationOperation::TransferMember { user_id, .. } if user_id == "one"));
    }

    // 验证仅改姓名的请求可缺省旧管理字段，往返序列化后仍省略这些字段。
    #[test]
    fn current_profile_payload_omits_retired_fields() {
        let value = serde_json::from_str::<PersonProfileChange>(
            r#"{
            "user_id":"one", "expected_name":"旧姓名", "name":"新姓名", "org_unit_id":null,
            "expected_role_ids":null, "role_ids":null
        }"#,
        )
        .unwrap();
        assert!(value.operations().unwrap().is_empty());
        let encoded = serde_json::to_value(&value).unwrap();
        assert!(encoded.get("remove_management_ids").is_none());
        assert!(encoded.get("add_management").is_none());
    }

    // 验证空改动和旧管理关系写入均被拒绝，历史移除与新增载荷仍可序列化往返。
    #[test]
    fn empty_and_legacy_management_writes_fail_but_history_roundtrips() {
        let mut value = change();
        assert!(value.operations().is_err());
        value.name = Some("新姓名".into());
        assert!(value.operations().unwrap().is_empty());

        value.remove_management_ids.push("legacy".into());
        assert!(value.operations().is_err());
        let encoded = serde_json::to_string(&value).unwrap();
        assert_eq!(serde_json::from_str::<PersonProfileChange>(&encoded).unwrap(), value);

        value.remove_management_ids.clear();
        value.add_management.push(PersonManagementChange {
            role_id: "old".into(),
            org_unit_id: "dept".into(),
            include_descendants: false,
            valid_to: None,
        });
        assert!(value.operations().is_err());
        let encoded = serde_json::to_string(&value).unwrap();
        assert_eq!(serde_json::from_str::<PersonProfileChange>(&encoded).unwrap(), value);
    }
}
