//! 审批人来源与组织负责人解析合同。
use serde::{Deserialize, Serialize};

use crate::{Error, Result};

/// 定义阶段的审批人来源，人员列表仅用于指定人员。
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AssigneeSource {
    #[default]
    Specified,
    ApplicantTeamLeader,
    ApplicantDepartmentLeader,
}
impl AssigneeSource {
    /// 返回写入 BPM 的不透明规则键。
    /// # 参数
    /// 无。
    /// # 返回
    /// 指定人员为空，负责人来源返回稳定键。
    /// # 错误
    /// 无。
    pub fn rule(self) -> Option<&'static str> {
        match self {
            Self::Specified => None,
            Self::ApplicantTeamLeader => Some("APPLICANT_TEAM_LEADER"),
            Self::ApplicantDepartmentLeader => Some("APPLICANT_DEPARTMENT_LEADER"),
        }
    }
    /// 从持久化规则读取受支持来源。
    /// # 参数
    /// `rule` 为定义规则。
    /// # 返回
    /// 返回对应来源。
    /// # 错误
    /// 未知规则失败关闭。
    pub fn from_rule(rule: Option<&str>) -> Result<Self> {
        match rule {
            None => Ok(Self::Specified),
            Some("APPLICANT_TEAM_LEADER") => Ok(Self::ApplicantTeamLeader),
            Some("APPLICANT_DEPARTMENT_LEADER") => Ok(Self::ApplicantDepartmentLeader),
            _ => Err(Error::ValidationError("审批人来源不受支持，请检查流程配置".into())),
        }
    }
    /// 返回配置阶段可读标签。
    /// # 参数
    /// 无。
    /// # 返回
    /// 中文来源标签。
    /// # 错误
    /// 无。
    pub fn label(self) -> &'static str {
        match self {
            Self::Specified => "指定人员",
            Self::ApplicantTeamLeader => "申请人的团队负责人",
            Self::ApplicantDepartmentLeader => "申请人的部门负责人",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::service::approval::definition_dto::DefinitionNodeRequest;
    #[test]
    fn dynamic_requests_do_not_require_or_accept_named_people() {
        let request: DefinitionNodeRequest = serde_json::from_value(serde_json::json!({
            "node_name": "团队审核", "display_order": 1, "assignee_source": "APPLICANT_TEAM_LEADER"
        }))
        .unwrap();
        assert!(request.clone().prepare().is_ok());
        let mut mixed = request.clone();
        mixed.assignee_user_id = "u1".into();
        assert!(mixed.prepare().is_err());
        let mut mixed = request;
        mixed.additional_assignee_user_ids.push("u2".into());
        assert!(mixed.prepare().is_err());
        let old: DefinitionNodeRequest = serde_json::from_value(serde_json::json!({
            "node_name": "指定审核", "display_order": 1, "assignee_user_id": "u1"
        }))
        .unwrap();
        assert_eq!(old.prepare().unwrap().assignee_source, AssigneeSource::Specified);
        assert!(AssigneeSource::from_rule(Some("UNKNOWN")).is_err());
    }
}
