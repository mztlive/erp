//! 内建岗位模板查询和显式生成协议。
use serde::{Deserialize, Serialize};

use crate::entity::role_template::{BuiltinRoleState, BuiltinRoleTemplate};

/// 生成内建岗位前供管理页审核的模板目录和策略版本。
#[derive(Debug, Serialize)]
pub struct BuiltinRoleCatalog {
    /// 目录检查资格时的策略版本，生成时提交此版本。
    pub policy_version: u64,
    /// 服务端固定模板及各模板的当前状态。
    pub templates: Vec<BuiltinRoleOption>,
}

/// 模板目录中的单项，供界面展示权限、生成状态及可用操作。
#[derive(Debug, Serialize)]
pub struct BuiltinRoleOption {
    /// 服务端定义的模板内容，序列化时平铺到目录项。
    #[serde(flatten)]
    pub template: BuiltinRoleTemplate,
    /// 数据库中该固定角色身份的当前状态，含已删除记录。
    pub state: BuiltinRoleState,
    /// 已有角色当前显示名；还没有对应记录时为空。
    pub existing_name: Option<String>,
    /// 模板尚无记录且操作者可创建角色并授予全部模板权限时为真；提交仍须校验。
    pub can_generate: bool,
}

/// 提交所选服务端模板，不接收客户端指定的权限或角色属性。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GenerateBuiltinRolesRequest {
    /// 选中且待整批处理的服务端模板 ID。
    pub template_ids: Vec<String>,
    /// 预览时读取的策略版本；版本不一致时需重新加载目录。
    pub expected_policy_version: u64,
}

/// 返回单个岗位的生成结果；已有身份不覆盖原配置或状态。
#[derive(Debug, Serialize)]
pub struct GeneratedBuiltinRole {
    /// 新建或保留的角色 ID，也是模板的固定身份。
    pub id: String,
    /// 新建角色的模板名称，或保留角色的现有名称。
    pub name: String,
    /// 本次是否新建该角色；已有记录时为假。
    pub created: bool,
    /// 操作后的角色状态；未新建时沿用既有状态。
    pub state: BuiltinRoleState,
}

#[cfg(test)]
mod tests {
    use super::*;

    // 验证生成协议要求预览版本，并拒绝独立提交权限。
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
