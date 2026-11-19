use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::entity::auth::validate_admin_password;
use crate::entity::{AccountCore, AccountStatus, LoginAccount};
use crate::error::{Error, Result};

/// 管理端新建管理员的账号资料及初始角色集。
#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct CreateAdminParams {
    /// 待创建的登录账号，入参长度为 3–32 个字符。
    #[validate(length(min = 3, max = 32, message = "账号长度必须在3-32个字符之间"))]
    pub account: String,
    /// 初始明文密码，保留空白，长度为 6–32 个字符。
    #[validate(custom(function = "validate_admin_password"))]
    pub password: String,
    /// 管理端显示名称，由账号领域规则校验并规范化。
    pub name: String,
    /// 初始角色 ID 集合，至少一个，须在操作人可授予范围内。
    #[validate(length(min = 1, message = "至少选择一个角色"))]
    pub role_ids: Vec<String>,
}

/// 服务启动引导和 `init-admin` 运维初始化共用的超级管理员参数。
#[derive(Debug, Serialize, Deserialize)]
pub struct InitializeSuperAdminParams {
    /// 用于查找或新建管理员的登录账号。
    pub account: String,
    /// 创建或修复时设置的明文密码，保留空白，长度为 6–32 个字符。
    pub password: String,
    /// 创建或修复时设置的显示名称，去除首尾空白后必填。
    pub name: String,
}

impl InitializeSuperAdminParams {
    /// 由启动配置或运维输入组装必填账号、密码和名称，校验留给初始化入口。
    pub fn new(account: impl Into<String>, password: impl Into<String>, name: impl Into<String>) -> Self {
        Self { account: account.into(), password: password.into(), name: name.into() }
    }

    /// 初始化读写前规范化登录账号和名称的首尾空白，返回处理后的账号、原始密码及名称。
    /// 账号非法、密码少于 6 或多于 32 个字符、名称为空时返回校验错误。
    pub(super) fn into_validated_parts(self) -> Result<(LoginAccount, String, String)> {
        let Self { account, password, name } = self;
        let account =
            LoginAccount::new(account).map_err(|_| Error::ValidationError("超级管理员账号不合法".into()))?;
        ensure_admin_password_length(&password, "超级管理员")?;

        let name = name.trim().to_string();
        if name.is_empty() {
            return Err(Error::ValidationError("超级管理员名称不能为空".into()));
        }

        Ok((account, password, name))
    }
}

/// 运维 `reset-password` 命令为已有管理员设置新密码的入参。
#[derive(Debug, Serialize, Deserialize)]
pub struct ResetAdminPasswordParams {
    /// 要重置密码的管理员登录账号。
    pub account: String,
    /// 待设置的明文密码，保留空白，长度为 6–32 个字符。
    pub password: String,
}

impl ResetAdminPasswordParams {
    /// 组装重置命令提供的必填账号和新密码，校验留给重置入口。
    pub fn new(account: impl Into<String>, password: impl Into<String>) -> Self {
        Self { account: account.into(), password: password.into() }
    }

    /// 重置密码查库前校验入参，返回去除首尾空白的登录账号与原始明文密码。
    /// 账号非法或密码长度超出 6–32 个字符时返回校验错误。
    pub(super) fn into_validated_parts(self) -> Result<(LoginAccount, String)> {
        let account =
            LoginAccount::new(self.account).map_err(|_| Error::ValidationError("管理员账号不合法".into()))?;
        ensure_admin_password_length(&self.password, "管理员")?;
        Ok((account, self.password))
    }
}

// 初始化与重置共用管理员密码规则：保留原值，按 6–32 个字符校验，失败时用 `label` 标明错误主体。
fn ensure_admin_password_length(password: &str, label: &str) -> Result<()> {
    validate_admin_password(password)
        .map_err(|_| Error::ValidationError(format!("{label}密码长度必须在6-32个字符之间")))
}

/// 管理端更新管理员的可选变更，缺省字段保留原值。
#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct UpdateAdminParams {
    /// 目标管理员 ID，由调用方从请求路径设置。
    #[serde(default, skip_deserializing)]
    pub id: String,
    /// 拟更新的显示名称，由账号领域规则校验并规范化。
    pub name: Option<String>,
    /// 拟设置的明文密码，保留空白，长度为 6–32 个字符。
    #[validate(custom(function = "validate_admin_password"))]
    pub password: Option<String>,
    /// 提供时替换整个角色集，且至少包含一个角色 ID。
    #[validate(length(min = 1, message = "至少选择一个角色"))]
    pub role_ids: Option<Vec<String>>,
    /// 拟变更的状态：`active` 启用，`suspended` 停用，`archived` 归档。
    pub status: Option<AccountStatus>,
}

impl UpdateAdminParams {
    /// 组装更新请求时先设置必填目标 ID，各可选变更初始均未指定。
    pub fn new(id: impl Into<String>) -> Self {
        Self { id: id.into(), name: None, password: None, role_ids: None, status: None }
    }

    /// 组装综合更新请求时指定完整替换角色集；至少一个角色的约束在用例入口校验。
    pub fn with_role_ids(mut self, role_ids: Vec<String>) -> Self {
        self.role_ids = Some(role_ids);
        self
    }
}

/// 管理端单独替换管理员角色的入参。
#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct UpdateAdminRoleParams {
    /// 目标管理员 ID，由调用方从请求路径设置。
    #[serde(default, skip_deserializing)]
    pub id: String,
    /// 替换后的完整角色 ID 集合，至少一个。
    #[validate(length(min = 1, message = "至少选择一个角色"))]
    pub role_ids: Vec<String>,
}

impl UpdateAdminRoleParams {
    /// 组装单独的换角色请求时设置目标 ID，角色集初始为空，提交前须填入角色。
    pub fn new(id: impl Into<String>) -> Self {
        Self { id: id.into(), role_ids: Vec::new() }
    }

    /// 为单独的换角色请求填入完整目标角色集，提交时再校验非空。
    pub fn with_role_ids(mut self, role_ids: Vec<String>) -> Self {
        self.role_ids = role_ids;
        self
    }
}

/// 管理员列表中的账号资料及直接绑定角色。
#[derive(Debug, Serialize)]
pub struct AdminItem {
    /// 管理员账号 ID。
    pub id: String,
    /// 管理员登录账号。
    pub account: String,
    /// 管理端显示名称。
    pub name: String,
    /// 当前账号状态；停用会阻止登录并使已有会话失效。
    pub status: AccountStatus,
    /// 当前直接绑定的 Casbin 角色 ID，未绑定时为空。
    pub role_ids: Vec<String>,
    /// 账号创建时间戳。
    pub created_at: u64,
}

impl AdminItem {
    /// 组装列表项时合并统一账号实体与同一快照读出的角色，只投影展示所需资料。
    pub(super) fn from_account(account: AccountCore, role_ids: Vec<String>) -> Self {
        Self {
            id: account.base.id,
            account: account.secret.into_account(),
            name: account.name,
            status: account.status,
            role_ids,
            created_at: account.base.created_at,
        }
    }
}

#[cfg(test)]
mod tests {
    use entity_core::BaseModel;
    use erp_core::AccountKind;
    use serde_json::json;
    use validator::Validate;

    use super::{
        AdminItem, CreateAdminParams, InitializeSuperAdminParams, ResetAdminPasswordParams,
        UpdateAdminParams, UpdateAdminRoleParams,
    };
    use crate::entity::{AccountCore, AccountStatus, LoginAccount, Secret};

    // 为列表序列化测试构造指定状态的管理员，并固定账号 ID 和创建时间。
    fn sample_admin(status: AccountStatus) -> AccountCore {
        AccountCore {
            base: BaseModel { id: "admin-1".to_string(), created_at: 42, ..Default::default() },
            secret: Secret::new(LoginAccount::new("root").unwrap(), "secret").unwrap(),
            name: "Root Admin".to_string(),
            kind: AccountKind::Admin,
            status,
            email: None,
            phone: None,
            avatar: None,
        }
    }

    // 验证管理员列表项的序列化字段、取值及角色列表符合接口约定。
    #[test]
    fn admin_item_preserves_the_exact_json_contract() {
        let value = serde_json::to_value(AdminItem::from_account(
            sample_admin(AccountStatus::Active),
            vec!["role-root".to_string()],
        ))
        .unwrap();

        assert_eq!(
            value,
            json!({
                "id": "admin-1",
                "account": "root",
                "name": "Root Admin",
                "status": "active",
                "role_ids": ["role-root"],
                "created_at": 42,
            })
        );
    }

    // 验证停用管理员在列表中序列化为 `suspended`。
    #[test]
    fn admin_item_exposes_suspended_status() {
        let value =
            serde_json::to_value(AdminItem::from_account(sample_admin(AccountStatus::Suspended), Vec::new()))
                .unwrap();

        assert_eq!(value["status"], "suspended");
    }

    // 验证更新请求接受定义内的 `suspended`，拒绝未定义的 `disabled`。
    #[test]
    fn update_admin_params_accept_only_known_status_values() {
        let params: UpdateAdminParams = serde_json::from_value(json!({ "status": "suspended" })).unwrap();
        assert_eq!(params.status, Some(AccountStatus::Suspended));

        assert!(serde_json::from_value::<UpdateAdminParams>(json!({ "status": "disabled" })).is_err());
    }

    // 验证初始化去除账号和名称的首尾空白，同时保留密码中的空白。
    #[test]
    fn super_admin_params_normalize_account_and_name_but_preserve_password() {
        let params = InitializeSuperAdminParams::new(" root ", " secret ", " Platform Admin ");
        let (account, password, name) = params.into_validated_parts().unwrap();
        assert_eq!(account.as_str(), "root");
        assert_eq!(password, " secret ");
        assert_eq!(name, "Platform Admin");
    }

    // 验证初始化拒绝全空白账号、空密码和全空白名称。
    #[test]
    fn super_admin_params_reject_invalid_required_values() {
        for params in [
            InitializeSuperAdminParams::new(" ", "password", "Platform Admin"),
            InitializeSuperAdminParams::new("root", String::new(), "Platform Admin"),
            InitializeSuperAdminParams::new("root", "password", " "),
        ] {
            assert!(params.into_validated_parts().is_err());
        }
    }

    // 验证重置时规范化账号、保留密码原值，并拒绝空账号和空密码。
    #[test]
    fn reset_admin_password_params_preserve_password_and_reject_invalid_values() {
        let (account, password) =
            ResetAdminPasswordParams::new(" admin01 ", " secret ").into_validated_parts().unwrap();
        assert_eq!(account.as_str(), "admin01");
        assert_eq!(password, " secret ");

        for params in [
            ResetAdminPasswordParams::new(" ", "password"),
            ResetAdminPasswordParams::new("admin01", String::new()),
        ] {
            assert!(params.into_validated_parts().is_err());
        }
    }

    // 验证新建、更新、初始化和重置按同一字符数规则校验密码，保留空白并允许更新时省略密码。
    #[test]
    fn all_admin_password_inputs_share_the_same_rule() {
        for (password, valid) in [
            ("short".to_string(), false),
            (" secret ".into(), true),
            ("密".repeat(32), true),
            ("密".repeat(33), false),
        ] {
            let create = CreateAdminParams {
                account: "admin01".into(),
                password: password.clone(),
                name: "Admin".into(),
                role_ids: vec!["role-1".into()],
            };
            let update =
                UpdateAdminParams { password: Some(password.clone()), ..UpdateAdminParams::new("admin-1") };
            assert_eq!(create.validate().is_ok(), valid);
            assert_eq!(update.validate().is_ok(), valid);

            let init =
                InitializeSuperAdminParams::new("admin01", password.clone(), "Admin").into_validated_parts();
            let reset = ResetAdminPasswordParams::new("admin01", password.clone()).into_validated_parts();
            assert_eq!(init.is_ok(), valid);
            assert_eq!(reset.is_ok(), valid);
            if valid {
                assert_eq!(init.unwrap().1, password);
                assert_eq!(reset.unwrap().1, password);
            }
        }

        assert!(UpdateAdminParams::new("admin-1").validate().is_ok());
    }

    // 验证新建和更新拒绝过短密码，单独换角色拒绝空角色集。
    #[test]
    fn admin_write_params_enforce_password_and_role_limits() {
        let create = CreateAdminParams {
            account: "admin01".to_string(),
            password: "short".to_string(),
            name: "Admin".to_string(),
            role_ids: vec!["role-1".to_string()],
        };
        let update =
            UpdateAdminParams { password: Some("short".into()), ..UpdateAdminParams::new("admin-1") };
        let roles = UpdateAdminRoleParams::new("admin-1");

        assert!(create.validate().is_err());
        assert!(update.validate().is_err());
        assert!(roles.validate().is_err());
    }

    // 验证构造方法填入必填账号或目标 ID，并能指定更新请求的角色集。
    #[test]
    fn admin_params_constructors_set_required_fields() {
        let init = InitializeSuperAdminParams::new("root", "password", "Platform Admin");
        assert_eq!(init.account, "root");
        let reset = ResetAdminPasswordParams::new("admin01", "password");
        assert_eq!(reset.account, "admin01");
        let update = UpdateAdminParams {
            name: Some("Admin".into()),
            ..UpdateAdminParams::new("admin-1").with_role_ids(vec!["r-1".to_string()])
        };
        assert_eq!(update.id, "admin-1");
        assert_eq!(update.name.as_deref(), Some("Admin"));
        assert_eq!(update.role_ids, Some(vec!["r-1".to_string()]));
        let roles = UpdateAdminRoleParams::new("admin-1").with_role_ids(vec!["r-1".to_string()]);
        assert_eq!(roles.role_ids, vec!["r-1".to_string()]);
    }
}
