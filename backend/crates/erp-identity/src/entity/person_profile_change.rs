//! 人员资料的原子变更：限定单人，按差异更新组织关系。
use std::collections::BTreeSet;

use erp_core::common::time::Instant;
use serde::{Deserialize, Serialize};

use super::organization_change::OrganizationOperation;
use crate::{Error, Result, RoleIdSet};

/// 仅供旧审计载荷反序列化的部门管理关系。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct PersonManagementChange {
    pub role_id: String,
    pub org_unit_id: String,
    pub include_descendants: bool,
    pub valid_to: Option<Instant>,
}

/// 姓名、完整角色集合与所属部门的一次原子修改。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PersonProfileChange {
    pub user_id: String,
    pub expected_name: String,
    pub name: Option<String>,
    pub expected_role_ids: Option<Vec<String>>,
    pub role_ids: Option<Vec<String>>,
    pub org_unit_id: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) remove_management_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) add_management: Vec<PersonManagementChange>,
}

impl PersonProfileChange {
    /// 校验完整角色集的期望值；旧管理关系不约束角色修改。
    ///
    /// # 参数
    /// * `current` - 当前授权快照的人员角色。
    /// # 返回
    /// 角色集合与期望版本校验成功。
    /// # 错误
    /// 空角色集、缺失期望值或角色版本冲突时拒绝。
    pub fn validate_roles(&self, current: &[String]) -> Result<()> {
        let Some(roles) = &self.role_ids else {
            return Ok(());
        };
        RoleIdSet::parse_non_empty(roles.clone())?;
        let expected = self
            .expected_role_ids
            .as_ref()
            .ok_or_else(|| Error::ValidationError("修改角色必须提供原角色集合".into()))?;
        if RoleIdSet::parse(expected.clone())?.to_strings().into_iter().collect::<BTreeSet<_>>()
            != RoleIdSet::parse(current.to_vec())?.to_strings().into_iter().collect::<BTreeSet<_>>()
        {
            return Err(Error::ConflictError("人员角色已变化，请刷新后重新编辑".into()));
        }
        Ok(())
    }

    /// 展开单人的所属部门变更，旧管理字段只允许存在于历史审计。
    ///
    /// # 参数
    /// 无。
    /// # 返回
    /// 零个或一个调岗命令。
    /// # 错误
    /// 空人员、空修改及停用的管理关系修改均拒绝。
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
    /// 角色保存不得覆盖过期快照，也不能生成空角色集。
    #[test]
    fn role_change_checks_original_set_and_accepts_role_only_change() {
        let mut value = change();
        value.role_ids = Some(vec!["new".into()]);
        let current = vec!["old".into()];
        assert!(value.validate_roles(&current).is_err());
        value.expected_role_ids = Some(vec!["other".into()]);
        assert!(matches!(value.validate_roles(&current), Err(Error::ConflictError(_))));
        value.expected_role_ids = Some(current.clone());
        assert!(value.validate_roles(&current).is_ok());
        assert!(value.operations().unwrap().is_empty());
        value.expected_role_ids = Some(vec!["second".into(), "old".into()]);
        assert!(value.validate_roles(&["old".into(), "second".into()]).is_ok());
        value.expected_role_ids = Some(current.clone());
        value.role_ids = Some(vec![]);
        assert!(value.validate_roles(&current).is_err());
    }

    #[test]
    fn profile_expands_only_target_person() {
        let mut value = change();
        value.org_unit_id = Some("sales".into());
        let ops = value.operations().unwrap();
        assert_eq!(ops.len(), 1);
        assert!(matches!(&ops[0], OrganizationOperation::TransferMember { user_id, .. } if user_id == "one"));
    }

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
