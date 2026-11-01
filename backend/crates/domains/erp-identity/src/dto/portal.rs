//! 供应商账号管理与登录的字段允许列表。

use erp_core::AccountKind;
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::AccountCore;
use crate::entity::portal::{PortalActor, PortalBinding, PortalRole};

/// 内部开通供应商实名账号的数据，不接收内部角色或组织。
#[derive(Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct PortalAccountCreate {
    #[validate(length(min = 3, max = 32))]
    pub account: String,
    #[validate(length(min = 1, max = 64))]
    pub name: String,
    #[validate(length(min = 6, max = 32))]
    pub password: String,
    pub role: PortalRole,
}

/// 内部管理固定岗位和启停；第一版禁止跨供应商重新绑定。
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PortalAccountUpdate {
    pub expected_account_version: u64,
    pub expected_binding_version: u64,
    pub role: PortalRole,
    pub active: bool,
}

/// 门户登录参数，身份类型固定由服务端选择。
#[derive(Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct PortalLoginPayload {
    #[validate(length(min = 3, max = 32))]
    pub account: String,
    #[validate(length(min = 6, max = 32))]
    pub password: String,
}

/// 自助密码修改请求，不允许修改供应商绑定或门户岗位。
#[derive(Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct PortalPasswordUpdate {
    #[validate(length(min = 6, max = 32))]
    pub old_password: String,
    #[validate(length(min = 6, max = 32))]
    pub new_password: String,
}

/// 不携带密码及内部权限事实的账号管理视图。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PortalAccountView {
    pub account_id: String,
    pub account: String,
    pub name: String,
    pub supplier_id: String,
    pub role: PortalRole,
    pub active: bool,
    pub account_version: u64,
    pub binding_version: u64,
}

impl PortalAccountView {
    /// 将同一账号与绑定装配为管理视图。
    ///
    /// # 参数
    /// `account`、`binding` 为同一外部账号的正式事实。
    /// # 返回
    /// 返回安全展示字段，失效账号仍可由内部查看。
    /// # 错误
    /// 无。
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

impl From<PortalActor> for PortalAccountView {
    /// 将已验证身份映射为不含凭证的账号视图。
    fn from(actor: PortalActor) -> Self {
        Self {
            account_id: actor.account_id,
            account: actor.account,
            name: actor.name,
            supplier_id: actor.supplier_id,
            role: actor.role,
            active: true,
            account_version: actor.account_version,
            binding_version: actor.binding_version,
        }
    }
}

#[cfg(test)]
mod tests {
    use validator::Validate;

    use super::{PortalAccountCreate, PortalLoginPayload, PortalPasswordUpdate};

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
