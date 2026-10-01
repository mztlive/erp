//! 内建岗位模板查询和显式生成协议。
use serde::{Deserialize, Serialize};

use crate::entity::role_template::{BuiltinRoleState, BuiltinRoleTemplate};

/// 模板目录及授权预览版本。
#[derive(Debug, Serialize)]
pub struct BuiltinRoleCatalog {
    pub policy_version: u64,
    pub templates: Vec<BuiltinRoleOption>,
}

/// 岗位模板及数据库中的当前生成状态。
#[derive(Debug, Serialize)]
pub struct BuiltinRoleOption {
    #[serde(flatten)]
    pub template: BuiltinRoleTemplate,
    pub state: BuiltinRoleState,
    pub existing_name: Option<String>,
    pub can_generate: bool,
}

/// 只允许提交服务器模板标识，不接受客户端权限或角色属性。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GenerateBuiltinRolesRequest {
    pub template_ids: Vec<String>,
    pub expected_policy_version: u64,
}

/// 每个岗位的生成结果；未创建项保留既有角色的状态与配置。
#[derive(Debug, Serialize)]
pub struct GeneratedBuiltinRole {
    pub id: String,
    pub name: String,
    pub created: bool,
    pub state: BuiltinRoleState,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 客户端不能注入权限或省略预览版本。
    #[test]
    fn generation_request_rejects_permission_injection_and_missing_version() {
        assert!(
            serde_json::from_value::<GenerateBuiltinRolesRequest>(serde_json::json!({
                "template_ids": ["role-sales"], "expected_policy_version": 1
            }))
            .is_ok()
        );
        for value in [
            serde_json::json!({"template_ids": ["role-sales"]}),
            serde_json::json!({"template_ids": ["role-sales"], "expected_policy_version": 1, "permissions": ["*:*"]}),
        ] {
            assert!(serde_json::from_value::<GenerateBuiltinRolesRequest>(value).is_err());
        }
    }
}
