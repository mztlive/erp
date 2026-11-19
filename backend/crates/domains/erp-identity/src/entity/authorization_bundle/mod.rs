//! 可移植的显式授权配置；仅声明自身管理的对象和替换边界。
mod export;
mod facts;
mod normalize;
pub(crate) use normalize::normalize_id;
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

/// 声明可移植的显式授权配置；未列出的对象和动作保留当前配置。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PolicyDocument {
    /// 文件格式版本，独立于服务端授权策略的修订号。
    pub version: PolicyVersion,
    /// 本批管理的角色及其直接功能权限。
    pub roles: Vec<PolicyRole>,
    /// 本批管理的后台人员直接角色绑定。
    pub bindings: Vec<PolicyBinding>,
    /// 本批管理的人员、资源和动作范围配置。
    pub data_scopes: Vec<PolicyScope>,
}

/// 按稳定角色 ID 声明名称和直接功能权限；改名不改变角色身份。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PolicyRole {
    /// 持久化角色 ID；新建角色必须使用 `policy-` 前缀。
    pub id: RoleId,
    /// 本批写入的角色显示名称，按角色实体规则校验。
    pub name: String,
    /// 对直接权限取并集或整组替换；两种模式都采用文件中的名称。
    pub mode: PolicyMode,
    /// 待合并或替换的精确权限及通配模式，均须覆盖已登记权限。
    pub permissions: Vec<Permission>,
}

/// 为已有后台人员声明直接角色绑定。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PolicyBinding {
    /// 绑定目标的后台账号 ID。
    pub user_id: String,
    /// 对直接绑定角色取并集或整组替换。
    pub mode: PolicyMode,
    /// 本批追加或完整替换的角色 ID；空替换清除直接绑定。
    pub role_ids: Vec<RoleId>,
}

/// 完整声明选定人员和动作的追加数据范围；本人基础由服务端政策决定。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PolicyScope {
    /// 接收范围配置的后台账号 ID。
    pub user_id: String,
    /// 登记人员范围规则的业务资源代码。
    pub resource: String,
    /// 本条替换的可配置动作；同一人员和资源的多个条目之间不得重叠。
    pub actions: Vec<String>,
    /// 仅接受 `replace`，逐动作替换原有追加范围。
    pub mode: PolicyMode,
    /// 各动作的追加授权条款；空列表清空选定动作的追加项。
    pub grants: Vec<PersonScopeGrant>,
    /// 是否明确允许选定动作的旧完整表达式转换为基础范围加追加授权；默认关闭。
    #[serde(default)]
    pub replace_legacy: bool,
}
