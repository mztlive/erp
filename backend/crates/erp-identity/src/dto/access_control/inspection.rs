//! 权限检查只返回判定与下一步，不返回其他人员的业务数据。
use serde::{Deserialize, Serialize};

use crate::access_control::{DataScope, ScopeTargetMode};
use crate::service::access_control::consumers::registration;
use crate::service::access_control::resolve::AuthorizedDataScope;
use crate::{Error, Result};

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
    /// # 参数
    /// 无。
    /// # 返回
    /// 输入合法时成功。
    /// # 错误
    /// 目标为空、过长或具体对象检查不受支持时拒绝。
    pub fn validate(&self) -> Result<()> {
        if self.user_id.trim().is_empty() || self.user_id.len() > 128 {
            return Err(Error::ValidationError("请选择需要检查的人员".into()));
        }
        registration(&self.resource, &self.action)?;
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
    /// 追加明确的检查层结果。
    /// # 参数
    /// * `layer` - 检查层。
    /// * `status` - passed、blocked 或 review。
    /// * `message` - 用户可读的原因与下一步。
    /// # 返回
    /// 无。
    /// # 错误
    /// 无。
    pub fn push(&mut self, layer: &str, status: &str, message: &str) {
        self.steps.push(AccessInspectionStep {
            layer: layer.into(),
            status: status.into(),
            message: message.into(),
        });
    }

    /// 从服务端实际解析结果说明配置，不推测业务对象是否可见。
    /// # 参数
    /// * `access` - 当前账号的同角色权限及范围解析结果。
    /// # 返回
    /// 包含操作权限、范围和个人限制的检查报告。
    /// # 错误
    /// 无。
    pub fn from_access(access: &AuthorizedDataScope) -> Self {
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
                "已解析角色默认或个人业务扩展范围；具体单据仍需按责任、个人限制及业务条件判断。",
            );
        } else {
            view.push("业务数据范围", "review", "没有产生有效角色范围。请检查对应角色的范围、所属部门及该角色的管理部门；合法历史读取需按具体单据判断。");
        }
        if access.scope.user_limit.is_some() {
            view.push("个人范围限制", "review", "存在个人限制，最终结果须与其求交；它不会授予额外权限。");
        } else {
            view.push("个人范围限制", "passed", "未配置个人范围限制，不额外收窄角色授权。");
        }
        view
    }
}

impl AccessInspectionView {
    /// 解释特定角色的组织关系依赖，不把关系本身作为授权。
    /// # 参数
    /// * `access` - 本次已解析上下文。
    /// * `rule` - 当前角色规则。
    /// * `role_name` - 提供该规则的角色名称。
    /// # 返回
    /// 缺失关系时向报告追加明确原因。
    /// # 错误
    /// 无；本方法只解释缺项，授权结论由公共解析器提供。
    pub fn explain_relation(&mut self, access: &AuthorizedDataScope, rule: &DataScope, role_name: &str) {
        if !rule.binding.applies(&access.resource, &access.action) {
            return;
        }
        let state = &access.organizations;
        let missing = match rule.binding.target_mode {
            Some(ScopeTargetMode::ManagedOrgs) => !state.management.iter().any(|grant| {
                !grant.base.is_deleted()
                    && grant.user_id == access.user_id
                    && grant.role_id == rule.subject_id
                    && grant.validity.contains(access.as_of)
                    && state.units.iter().any(|unit| unit.base.id == grant.org_unit_id && unit.enabled)
            }),
            Some(ScopeTargetMode::OwnOrg) => !state.memberships.iter().any(|member| {
                !member.base.is_deleted()
                    && member.user_id == access.user_id
                    && member.validity.contains(access.as_of)
                    && state.units.iter().any(|unit| unit.base.id == member.org_unit_id && unit.enabled)
            }),
            _ => false,
        };
        if missing {
            let relation = if rule.binding.target_mode == Some(ScopeTargetMode::ManagedOrgs) {
                "该角色对应的有效管理部门"
            } else {
                "有效所属部门"
            };
            self.push(
                "组织关系",
                "review",
                &format!(
                    "{role_name}使用部门范围，但未找到{relation}。请在人员资料中补齐；其他角色仍独立计算。"
                ),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    /// 创建无数据库依赖的真实解析上下文。
    fn access() -> AuthorizedDataScope {
        use std::collections::BTreeMap;

        use erp_core::common::time::Instant;

        use crate::access_control::ResolvedScope;
        use crate::entity::organization_change::OrganizationState;
        AuthorizedDataScope {
            user_id: "u".into(),
            resource: "sales_order".into(),
            action: "detail".into(),
            scope: ResolvedScope { role_clauses: vec![], user_limit: None },
            role_scopes: BTreeMap::new(),
            organizations: OrganizationState::default(),
            policy_version: 1,
            scope_version: "v1".into(),
            as_of: Instant::from_unix_secs(100),
        }
    }

    #[test]
    fn empty_role_scope_is_review_and_personal_cap_never_grants_access() {
        use crate::access_control::ScopeClause;
        let mut access = access();
        access.scope.user_limit = Some(ScopeClause { company: true, ..ScopeClause::default() });
        let view = AccessInspectionView::from_access(&access);
        assert_eq!(view.steps[1].status, "review");
        assert_eq!(view.steps[2].status, "review");
        assert_eq!(view.scope_version.as_deref(), Some("v1"));
        access.scope.role_clauses.push(ScopeClause { self_owned: true, ..ScopeClause::default() });
        let view = AccessInspectionView::from_access(&access);
        assert_eq!(view.steps[1].status, "passed");
        assert_eq!(view.steps[2].status, "review");
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
