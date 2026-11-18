//! 权限检查只返回判定与下一步，不返回其他人员的业务数据。
use serde::{Deserialize, Serialize};

use crate::entity::access_control::scope_spec::{self, ScopeSource};
use crate::service::access_control::resolve::AuthorizedDataScope;
#[cfg(test)]
use crate::service::access_control::resolve::DataScopeFacts;
use crate::{Error, Permission, Result};

/// 管理员发起的只读访问检查。
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AccessInspectionRequest {
    pub user_id: String,
    pub resource: String,
    pub action: String,
    pub object_id: Option<String>,
}

impl AccessInspectionRequest {
    /// 校验检查目标及已接线的资源动作。
    /// 输入合法时成功。
    ///
    /// 目标为空、过长或具体对象检查不受支持时拒绝。
    pub fn validate(&self) -> Result<()> {
        if self.user_id.trim().is_empty() || self.user_id.len() > 128 {
            return Err(Error::ValidationError("请选择需要检查的人员".into()));
        }

        scope_spec::kind(&Permission::parse(format!("{}:{}", self.resource, self.action))?)?;
        if let Some(id) = &self.object_id {
            if id.trim().is_empty() || id.len() > 128 {
                return Err(Error::ValidationError("请选择有效业务单据".into()));
            }
            if self.resource != "sales_order" || !matches!(self.action.as_str(), "detail" | "update") {
                return Err(Error::ValidationError(
                    "具体单据检查支持销售单查看和修改；其他操作请检查配置".into(),
                ));
            }
        }
        Ok(())
    }
}

/// 单层检查结果；review 表示尚需业务对象或命令内容才能判断。
#[derive(Serialize)]
pub struct AccessInspectionStep {
    pub layer: String,
    pub status: String,
    pub message: String,
}

/// 检查结果不作为后续业务请求的授权凭证。
#[derive(Default, Serialize)]
pub struct AccessInspectionView {
    pub steps: Vec<AccessInspectionStep>,
    pub scope_version: Option<String>,
    pub checked_at: Option<String>,
}

impl AccessInspectionView {
    /// 缺少对象或任务上下文时只报告已完成的检查，不给出对象授权通过。
    /// 动作通过、对象条件待检查的只读结果。
    ///
    /// 动作未接入时返回校验错误。
    ///
    /// `resource` 与 `action` 为已接入的动作。
    pub fn without_object(resource: &str, action: &str) -> Result<Self> {
        let description = scope_spec::action_description(resource, action)?;
        let mut view = Self::default();
        view.push("操作权限", "passed", "账号有效，启用角色提供了本操作权限。");

        let company = scope_spec::find_action(resource, action)
            .is_some_and(|spec| matches!(spec.source(), ScopeSource::Company));
        if company {
            view.push("岗位共享", "passed", description);
            view.push(
                "对象资格",
                "review",
                "尚未检查具体对象；维护权限、敏感信息、单据状态及任务责任仍须在业务操作时校验。",
            );
            return Ok(view);
        }

        view.push("业务授权", "review", description);
        view.push(
            "对象上下文",
            "review",
            "需在实际业务中核验来源单据、任务归属、版本和管理资格；此检查不代表可以访问或处理具体对象。",
        );
        Ok(view)
    }

    /// 追加明确的检查层结果。
    /// `status`：passed、blocked 或 review；`message`：用户可读的原因与下一步。
    pub fn push(&mut self, layer: &str, status: &str, message: &str) {
        self.steps.push(AccessInspectionStep {
            layer: layer.into(),
            status: status.into(),
            message: message.into(),
        });
    }

    // 从服务端实际解析结果说明配置，不推测业务对象是否可见。
    // 包含操作权限、范围和个人限制的检查报告。
    //
    // `access`：当前账号的同角色权限及范围解析结果。
    pub(crate) fn from_access(access: &AuthorizedDataScope) -> Self {
        let mut view = Self {
            scope_version: Some(access.scope_version.clone()),
            checked_at: Some(access.as_of.as_utc().to_rfc3339()),
            ..Self::default()
        };
        view.push("操作权限", "passed", "账号有效，至少一个启用角色提供了本操作权限。");

        if access.scope.has_role_scope() {
            view.push(
                "业务数据范围",
                "passed",
                "已解析此人本操作的数据范围；具体单据仍需满足业务状态及审批条件。",
            );
        } else {
            view.push(
                "业务数据范围",
                "review",
                "未产生有效人员范围，请检查人员资料中本业务本操作的范围及所属部门。",
            );
        }
        view
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_policy_explains_company_access_without_claiming_task_eligibility() {
        let view = AccessInspectionView::without_object("product", "list").unwrap();
        assert_eq!(view.steps[1].layer, "岗位共享");
        assert_eq!(view.steps[1].status, "passed");
        assert_eq!(view.steps[2].status, "review");
        assert!(view.scope_version.is_none());
    }

    #[test]
    fn contextual_policies_never_report_object_access_as_passed_or_blocked() {
        for (resource, action) in
            [("approval_instance", "decide"), ("cost_entry", "list"), ("integration_error_task", "create")]
        {
            let view = AccessInspectionView::without_object(resource, action).unwrap();
            assert_eq!(view.steps[0].status, "passed");
            assert!(view.steps[1..].iter().all(|step| step.status == "review"));
            assert!(view.scope_version.is_none());
        }
    }

    #[test]
    fn validates_registered_actions_and_object_checks() {
        let mut request = AccessInspectionRequest {
            user_id: "u".into(),
            resource: "sales_order".into(),
            action: "detail".into(),
            object_id: Some("o".into()),
        };
        assert!(request.validate().is_ok());
        request.action = "submit".into();
        assert!(request.validate().is_err());
        request.object_id = None;
        assert!(request.validate().is_ok());

        request.resource = "unknown".into();
        assert!(request.validate().is_err());
        request.user_id.clear();
        assert!(request.validate().is_err());
    }
    // 为范围检查单测创建真实授权解析上下文，不依赖数据库设施。
    fn access() -> AuthorizedDataScope {
        use erp_core::common::time::Instant;

        use crate::access_control::ResolvedScope;
        use crate::entity::organization_change::OrganizationState;
        AuthorizedDataScope::test_fixture(DataScopeFacts {
            scope: ResolvedScope { role_clauses: vec![], user_limit: None },
            organizations: OrganizationState::default(),
            policy_version: 1,
            scope_version: "v1".into(),
            as_of: Instant::from_unix_secs(100),
        })
    }

    #[test]
    fn empty_role_scope_is_review_and_personal_cap_never_grants_access() {
        use crate::access_control::ScopeClause;
        let mut access = access();
        access.facts.scope.user_limit = Some(ScopeClause { company: true, ..ScopeClause::default() });
        let view = AccessInspectionView::from_access(&access);
        assert_eq!(view.steps[1].status, "review");
        assert_eq!(view.steps.len(), 2);
        assert_eq!(view.scope_version.as_deref(), Some("v1"));

        access.facts.scope.role_clauses.push(ScopeClause { self_owned: true, ..ScopeClause::default() });
        let view = AccessInspectionView::from_access(&access);
        assert_eq!(view.steps[1].status, "passed");
        assert_eq!(view.steps.len(), 2);
    }

    #[test]
    fn request_rejects_unknown_fields_and_blank_object() {
        assert!(
            serde_json::from_value::<AccessInspectionRequest>(serde_json::json!({
                "user_id": "u", "resource": "sales_order", "action": "detail", "allow": true
            }))
            .is_err()
        );

        let request = AccessInspectionRequest {
            user_id: "u".into(),
            resource: "sales_order".into(),
            action: "detail".into(),
            object_id: Some(" ".into()),
        };
        assert!(request.validate().is_err());
    }
}
