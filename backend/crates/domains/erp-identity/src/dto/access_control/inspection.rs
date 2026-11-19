//! 只读权限检查：返回分层判定和下一步建议，业务数据留在原读取入口。
use serde::{Deserialize, Serialize};

use crate::entity::access_control::scope_spec::{self, ScopeSource};
use crate::service::access_control::resolve::AuthorizedDataScope;
use crate::{Error, Permission, Result};

/// 管理员发起的只读权限检查，拒绝未知请求字段。
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AccessInspectionRequest {
    /// 待检查的账号 ID；须含非空白内容，且最多 128 字节。
    pub user_id: String,
    /// 已登记的权限资源代码，例如 `sales_order`。
    pub resource: String,
    /// 该资源下已登记的动作代码，例如 `detail`。
    pub action: String,
    /// 可选单据 ID；须含非空白内容，且最多 128 字节。省略时只检查配置。
    pub object_id: Option<String>,
}

impl AccessInspectionRequest {
    /// 校验账号 ID、可选单据 ID 和资源动作登记。
    /// 提供单据 ID 时，仅接受销售单的 `detail` 和 `update`。
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

/// 权限检查中单个层级的判定和说明。
#[derive(Serialize)]
pub struct AccessInspectionStep {
    /// 检查层级的展示名称。
    pub layer: String,
    /// `passed` 为通过，`blocked` 为阻止，`review` 为待核实。缺少业务对象或命令上下文时保留待核实。
    pub status: String,
    /// 面向使用者的判定原因和下一步建议。
    pub message: String,
}

/// 只读权限检查报告；后续业务请求仍须重新授权。
#[derive(Default, Serialize)]
pub struct AccessInspectionView {
    /// 按追加顺序展示的分层结果。
    pub steps: Vec<AccessInspectionStep>,
    /// 实际解析的数据范围版本；未解析范围时为空。
    pub scope_version: Option<String>,
    /// 范围解析使用的时点，以 UTC 的 RFC 3339 文本表示；未解析范围时为空。
    pub checked_at: Option<String>,
}

impl AccessInspectionView {
    /// 调用方确认操作权限后，生成缺少对象或任务上下文的报告。
    /// 公司共享动作另标记岗位共享通过；对象条件一律待核实，未登记的动作返回校验错误。
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

    /// 按调用顺序追加已判定的结果，状态使用 `passed`、`blocked` 或 `review`，说明面向使用者。
    pub fn push(&mut self, layer: &str, status: &str, message: &str) {
        self.steps.push(AccessInspectionStep {
            layer: layer.into(),
            status: status.into(),
            message: message.into(),
        });
    }

    /// 根据服务端解析结果报告操作权限及范围版本。存在角色范围条款时标记范围通过，具体对象仍须检查。
    /// 个人上限仅收窄范围；没有角色范围条款时标记待核实。
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

    // 验证公司共享动作通过岗位共享检查，具体对象仍待核实，且无范围版本。
    #[test]
    fn shared_policy_explains_company_access_without_claiming_task_eligibility() {
        let view = AccessInspectionView::without_object("product", "list").unwrap();
        assert_eq!(view.steps[1].layer, "岗位共享");
        assert_eq!(view.steps[1].status, "passed");
        assert_eq!(view.steps[2].status, "review");
        assert!(view.scope_version.is_none());
    }

    // 验证依赖来源或任务的动作只确认操作权限，其余条件待核实且无范围版本。
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

    // 验证销售单查看支持单据检查，提交可做配置检查，未知资源会被拒绝。
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
    // 构造含固定版本和时点的范围解析结果，角色条款及个人上限起初为空。
    fn access() -> AuthorizedDataScope {
        use erp_core::common::time::Instant;

        use crate::access_control::ResolvedScope;
        use crate::entity::organization_change::OrganizationState;
        AuthorizedDataScope {
            scope: ResolvedScope { role_clauses: vec![], user_limit: None },
            organizations: OrganizationState::default(),
            policy_version: 1,
            scope_version: "v1".into(),
            as_of: Instant::from_unix_secs(100),
        }
    }

    // 验证只有个人上限时范围待核实，加入角色条款后通过，并保留范围版本。
    #[test]
    fn empty_role_scope_is_review_and_personal_cap_never_grants_access() {
        use crate::access_control::ScopeClause;
        let mut access = access();
        access.scope.user_limit = Some(ScopeClause { company: true, ..ScopeClause::default() });
        let view = AccessInspectionView::from_access(&access);
        assert_eq!(view.steps[1].status, "review");
        assert_eq!(view.steps.len(), 2);
        assert_eq!(view.scope_version.as_deref(), Some("v1"));

        access.scope.role_clauses.push(ScopeClause { self_owned: true, ..ScopeClause::default() });
        let view = AccessInspectionView::from_access(&access);
        assert_eq!(view.steps[1].status, "passed");
        assert_eq!(view.steps.len(), 2);
    }

    // 验证反序列化拒绝未知字段，参数校验拒绝纯空白单据 ID。
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
