//! 个人部门授权命令与读取信封。
use serde::{Deserialize, Serialize};

use crate::entity::access_control::personal_grant::{PersonalBusinessGrant, PersonalBusinessGrantData};

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreatePersonalGrantRequest {
    pub grant: PersonalBusinessGrantData,
    pub expected_policy_version: u64,
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RevokePersonalGrantRequest {
    pub version: u64,
    pub expected_policy_version: u64,
}

#[derive(Serialize)]
pub struct PersonalGrantView {
    #[serde(flatten)]
    pub grant: PersonalBusinessGrant,
    /// 当前同一有效角色仍提供的动作；空集合表示依据失效。
    pub active_actions: Vec<String>,
}

#[derive(Serialize)]
pub struct GrantBusinessOption {
    pub resource: String,
    pub actions: Vec<String>,
}

#[derive(Serialize)]
pub struct GrantRoleOption {
    pub id: String,
    pub name: String,
    pub resources: Vec<GrantBusinessOption>,
}

#[derive(Serialize)]
pub struct PersonalGrantListView {
    pub items: Vec<PersonalGrantView>,
    pub roles: Vec<GrantRoleOption>,
    pub policy_version: u64,
}
