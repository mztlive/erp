use serde::{Deserialize, Serialize};
use validator::Validate;

use entities::{AccountCore, LoginAccount};

use crate::errors::{Error, Result};

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct CreateAdminParams {
    #[validate(length(min = 3, max = 32, message = "账号长度必须在3-32个字符之间"))]
    pub account: String,
    #[validate(length(min = 6, max = 32, message = "密码长度必须在6-32个字符之间"))]
    pub password: String,
    pub name: String,
    #[validate(length(min = 1, message = "至少选择一个角色"))]
    pub role_ids: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct InitializeSuperAdminParams {
    pub account: String,
    pub password: String,
    pub name: String,
}

impl InitializeSuperAdminParams {
    /// 校验并规范化超级管理员初始化参数。
    ///
    /// 账号与名称会按各自领域规则去除首尾空白，密码保持原值。
    ///
    /// # 返回值
    /// 返回规范化后的登录账号、密码与管理员名称。
    ///
    /// # 错误
    /// 当账号不合法、密码长度不在 6-32 个字符内或名称为空时返回校验错误。
    pub(super) fn into_validated_parts(self) -> Result<(LoginAccount, String, String)> {
        let Self {
            account,
            password,
            name,
        } = self;
        let account =
            LoginAccount::new(account).map_err(|_| Error::ValidationError("超级管理员账号不合法".into()))?;
        if !(6..=32).contains(&password.chars().count()) {
            return Err(Error::ValidationError(
                "超级管理员密码长度必须在6-32个字符之间".into(),
            ));
        }

        let name = name.trim().to_string();
        if name.is_empty() {
            return Err(Error::ValidationError("超级管理员名称不能为空".into()));
        }

        Ok((account, password, name))
    }
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct UpdateAdminParams {
    #[serde(default, skip_deserializing)]
    pub id: String,
    pub name: Option<String>,
    #[validate(length(min = 6, max = 32, message = "密码长度必须在6-32个字符之间"))]
    pub password: Option<String>,
    #[validate(length(min = 1, message = "至少选择一个角色"))]
    pub role_ids: Option<Vec<String>>,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct UpdateAdminRoleParams {
    #[serde(default, skip_deserializing)]
    pub id: String,
    #[validate(length(min = 1, message = "至少选择一个角色"))]
    pub role_ids: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct AdminItem {
    pub id: String,
    pub account: String,
    pub name: String,
    pub role_ids: Vec<String>,
    pub created_at: u64,
}

impl AdminItem {
    /// 通过统一账号实体和角色构建管理员响应。
    ///
    /// # 参数
    /// * `account` - 管理员统一账号实体
    /// * `role_ids` - 管理员角色ID集合
    ///
    /// # 返回值
    /// 返回管理员响应结构
    pub(super) fn from_account(account: AccountCore, role_ids: Vec<String>) -> Self {
        Self {
            id: account.base.id,
            account: account.secret.into_account(),
            name: account.name,
            role_ids,
            created_at: account.base.created_at,
        }
    }
}

#[cfg(test)]
mod tests {
    use entities::{AccountCore, AccountKind, AccountStatus, BaseModel, LoginAccount, Secret};
    use serde_json::json;
    use validator::Validate;

    use super::{
        AdminItem, CreateAdminParams, InitializeSuperAdminParams, UpdateAdminParams, UpdateAdminRoleParams,
    };

    #[test]
    fn admin_item_preserves_the_exact_json_contract() {
        let account = AccountCore {
            base: BaseModel {
                id: "admin-1".to_string(),
                created_at: 42,
                ..Default::default()
            },
            secret: Secret::new(LoginAccount::new("root").unwrap(), "secret").unwrap(),
            name: "Root Admin".to_string(),
            kind: AccountKind::Admin,
            status: AccountStatus::Active,
            email: None,
            phone: None,
            avatar: None,
        };

        let value =
            serde_json::to_value(AdminItem::from_account(account, vec!["role-root".to_string()])).unwrap();

        assert_eq!(
            value,
            json!({
                "id": "admin-1",
                "account": "root",
                "name": "Root Admin",
                "role_ids": ["role-root"],
                "created_at": 42,
            })
        );
    }

    #[test]
    fn super_admin_params_normalize_account_and_name_but_preserve_password() {
        let params = InitializeSuperAdminParams {
            account: " root ".to_string(),
            password: " secret ".to_string(),
            name: " Platform Admin ".to_string(),
        };

        let (account, password, name) = params.into_validated_parts().unwrap();

        assert_eq!(account.as_str(), "root");
        assert_eq!(password, " secret ");
        assert_eq!(name, "Platform Admin");
    }

    #[test]
    fn super_admin_params_reject_invalid_required_values() {
        for params in [
            InitializeSuperAdminParams {
                account: " ".to_string(),
                password: "password".to_string(),
                name: "Platform Admin".to_string(),
            },
            InitializeSuperAdminParams {
                account: "root".to_string(),
                password: String::new(),
                name: "Platform Admin".to_string(),
            },
            InitializeSuperAdminParams {
                account: "root".to_string(),
                password: "password".to_string(),
                name: " ".to_string(),
            },
        ] {
            assert!(params.into_validated_parts().is_err());
        }
    }

    #[test]
    fn admin_write_params_enforce_password_and_role_limits() {
        let create = CreateAdminParams {
            account: "admin01".to_string(),
            password: "short".to_string(),
            name: "Admin".to_string(),
            role_ids: vec!["role-1".to_string()],
        };
        let update = UpdateAdminParams {
            id: "admin-1".to_string(),
            name: None,
            password: Some("short".to_string()),
            role_ids: None,
        };
        let roles = UpdateAdminRoleParams {
            id: "admin-1".to_string(),
            role_ids: Vec::new(),
        };

        assert!(create.validate().is_err());
        assert!(update.validate().is_err());
        assert!(roles.validate().is_err());
    }
}
