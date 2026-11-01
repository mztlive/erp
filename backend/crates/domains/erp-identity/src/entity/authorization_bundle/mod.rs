//! 可移植的显式授权配置；仅声明自身管理的对象和替换边界。
mod export;
mod facts;
mod normalize;
pub(crate) mod plan;
pub(crate) mod receipt;

use serde::{Deserialize, Serialize};

use crate::dto::person_scope::PersonScopeGrant;
use crate::entity::rbac::{Permission, RoleId};

/// 授权文件版本，未知版本在反序列化边界拒绝。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PolicyVersion {
    #[serde(rename = "1.0")]
    V1,
}

/// 集合的显式管理方式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PolicyMode {
    Merge,
    Replace,
}

/// 授权文件；未列对象不属于本文件的管理范围。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PolicyDocument {
    pub version: PolicyVersion,
    pub roles: Vec<PolicyRole>,
    pub bindings: Vec<PolicyBinding>,
    pub data_scopes: Vec<PolicyScope>,
}

/// 角色操作权限；名称不参与身份匹配。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PolicyRole {
    pub id: RoleId,
    pub name: String,
    pub mode: PolicyMode,
    pub permissions: Vec<Permission>,
}

/// 当前后台人员的真实角色集合。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PolicyBinding {
    pub user_id: String,
    pub mode: PolicyMode,
    pub role_ids: Vec<RoleId>,
}

/// 选定人员资源动作的完整追加范围。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PolicyScope {
    pub user_id: String,
    pub resource: String,
    pub actions: Vec<String>,
    pub mode: PolicyMode,
    pub grants: Vec<PersonScopeGrant>,
    #[serde(default)]
    pub replace_legacy: bool,
}
