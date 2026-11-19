//! 供应商账号管理与登录的字段允许列表。

use erp_core::AccountKind;
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::AccountCore;
use crate::entity::portal::{PortalBinding, PortalRole};

/// 内部开通供应商实名账号时使用；供应商由调用方确定，不接收内部角色或组织。
#[derive(Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct PortalAccountCreate {
    /// 供应商门户的登录账号，长度为 3 至 32 个字符。
    #[validate(length(min = 3, max = 32, message = "账号长度须为 3 至 32 个字符"))]
    pub account: String,
    /// 实名账号的展示名称，长度为 1 至 64 个字符。
    #[validate(length(min = 1, max = 64, message = "名称长度须为 1 至 64 个字符"))]
    pub name: String,
    /// 开通时设置的初始密码，长度为 6 至 32 个字符；仅用于生成凭证。
    #[validate(length(min = 6, max = 32, message = "密码长度须为 6 至 32 个字符"))]
    pub password: String,
    /// 开通时绑定的供应商门户岗位，不是后台角色。
    pub role: PortalRole,
}

/// 内部管理门户账号的岗位和启停状态，不允许跨供应商改绑。
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PortalAccountUpdate {
    /// 编辑时读取的账号版本，保存时用于检查账号并发变更。
    pub expected_account_version: u64,
    /// 编辑时读取的绑定版本，保存时用于检查绑定并发变更。
    pub expected_binding_version: u64,
    /// 更新后在原供应商下使用的门户岗位。
    pub role: PortalRole,
    /// 期望的启停状态，保存时同步更新账号与绑定。
    pub active: bool,
}

/// 供应商门户登录凭证；身份类型由服务端固定。
#[derive(Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct PortalLoginPayload {
    /// 门户登录时提交的账号，长度为 3 至 32 个字符。
    #[validate(length(min = 3, max = 32, message = "账号长度须为 3 至 32 个字符"))]
    pub account: String,
    /// 门户登录时提交的密码，长度为 6 至 32 个字符。
    #[validate(length(min = 6, max = 32, message = "密码长度须为 6 至 32 个字符"))]
    pub password: String,
}

/// 供应商自助改密请求，不允许同时修改供应商绑定或门户岗位。
#[derive(Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct PortalPasswordUpdate {
    /// 用于确认现有凭证的原密码，长度为 6 至 32 个字符。
    #[validate(length(min = 6, max = 32, message = "原密码长度须为 6 至 32 个字符"))]
    pub old_password: String,
    /// 换发的密码，使用与门户登录一致的 6 至 32 字符长度约束。
    #[validate(length(min = 6, max = 32, message = "新密码长度须为 6 至 32 个字符"))]
    pub new_password: String,
}

/// 用于内部管理和展示的供应商账号视图，不携带密码或内部权限事实。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PortalAccountView {
    /// 用于管理指定供应商账号的稳定 ID。
    pub account_id: String,
    /// 管理页显示的门户登录账号。
    pub account: String,
    /// 供应商人员的显示名称。
    pub name: String,
    /// 当前绑定的供应商 ID。
    pub supplier_id: String,
    /// 当前绑定的门户岗位。
    pub role: PortalRole,
    /// 仅供应商账号允许登录且绑定启用时为真。
    pub active: bool,
    /// 当前账号版本，编辑提交时用于检查并发变更。
    pub account_version: u64,
    /// 当前绑定版本，编辑提交时用于检查并发变更。
    pub binding_version: u64,
}

impl PortalAccountView {
    /// 开通、查询或更新后，将同一门户账号与绑定投影为管理视图；停用记录仍可展示。
    pub fn from_account(account: &AccountCore, binding: &PortalBinding) -> Self {
        Self {
            account_id: account.base.id.clone(),
            account: account.secret.account().to_string(),
            name: account.name.clone(),
            supplier_id: binding.supplier_id.clone(),
            role: binding.role,
            active: account.kind == AccountKind::Supplier && account.can_login() && binding.active,
            account_version: account.base.version,
            binding_version: binding.base.version,
        }
    }
}

#[cfg(test)]
mod tests {
    use validator::Validate;

    use super::{PortalAccountCreate, PortalLoginPayload, PortalPasswordUpdate};

    // 验证新密码与登录密码在长度边界采用同一规则，且原密码也受长度校验。
    #[test]
    fn changed_password_and_login_share_length_policy_at_each_boundary() {
        for len in [0, 1, 5, 6, 32, 33, 128] {
            let password = "x".repeat(len);
            let update =
                PortalPasswordUpdate { old_password: "123456".into(), new_password: password.clone() };
            let login = PortalLoginPayload { account: "supplier01".into(), password };
            assert_eq!(update.validate().is_ok(), (6..=32).contains(&len));
            assert_eq!(update.validate().is_ok(), login.validate().is_ok());
        }
        let update = PortalPasswordUpdate { old_password: "x".into(), new_password: "123456".into() };
        assert!(update.validate().is_err());
    }

    // 验证登录、开户和改密分别拒绝额外的账号类型、供应商 ID 和岗位字段。
    #[test]
    fn inputs_reject_client_identity_and_internal_role_injection() {
        assert!(
            serde_json::from_str::<PortalLoginPayload>(
                r#"{"account":"user01","password":"password123","account_kind":"admin"}"#
            )
            .is_err()
        );
        assert!(serde_json::from_str::<PortalAccountCreate>(r#"{"account":"user01","name":"name","password":"password123","role":"maintainer","supplier_id":"other"}"#).is_err());
        assert!(
            serde_json::from_str::<PortalPasswordUpdate>(
                r#"{"old_password":"password123","new_password":"password456","role":"maintainer"}"#
            )
            .is_err()
        );
    }
}
