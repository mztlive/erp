use database::DatabaseExt;
use entities::{AccountCore, AccountKind, Permission};
use mongodb::Database;
use serde::Serialize;

use crate::account_support::account_of_kind;
use crate::errors::Result;
use crate::iam::{self, SharedRbacService};

/// 当前账号信息响应结构。
#[derive(Debug, Serialize)]
pub struct AccountProfile {
    #[serde(rename = "userid")]
    pub user_id: String,
    pub account: String,
    pub name: String,
    pub email: Option<String>,
    pub phone: Option<String>,
    pub avatar: Option<String>,
    pub subject: String,
    pub role_ids: Vec<String>,
    pub permissions: Vec<Permission>,
    pub account_kind: AccountKind,
    pub store_id: Option<String>,
}

/// 账号信息服务。
///
/// 提供当前账号信息的读取能力。
pub struct AccountProfileService {
    db: Database,
    rbac: SharedRbacService,
}

impl AccountProfileService {
    /// 使用共享 RBAC 服务创建账号信息服务实例。
    ///
    /// # 参数
    /// * `db` - 数据库实例
    /// * `rbac` - 共享 Casbin RBAC 服务
    ///
    /// # 返回值
    /// 返回账号信息服务实例
    pub fn new(db: Database, rbac: SharedRbacService) -> Self {
        Self { db, rbac }
    }

    /// 获取当前账号信息。
    ///
    /// # 参数
    /// * `user_id` - 账号ID
    /// * `account_kind` - 账号类型
    ///
    /// # 返回值
    /// 返回账号信息
    ///
    /// # 错误
    /// 当账号不存在时返回错误。
    pub async fn account_profile(&self, user_id: &str, account_kind: AccountKind) -> Result<AccountProfile> {
        let account = self.load_account(user_id, account_kind, "管理员不存在").await?;
        let role_ids = self.role_ids_for_account(&account).await?;
        let permissions = self.account_permissions(&account).await?;
        let avatar = Self::normalized_avatar(account.avatar.as_deref());
        Ok(Self::build_profile(account, role_ids, permissions, avatar))
    }

    /// 查询账号绑定的角色ID集合。
    ///
    /// # 参数
    /// * `account` - 账号实体
    ///
    /// # 返回值
    /// 返回角色ID集合
    async fn role_ids_for_account(&self, account: &AccountCore) -> Result<Vec<String>> {
        self.rbac.role_ids(account.kind, account.base.id.as_str()).await
    }

    /// 按账号类型加载账号并校验类型。
    ///
    /// # 参数
    /// * `user_id` - 账号ID
    /// * `account_kind` - 账号类型
    /// * `not_found_message` - 错误信息
    ///
    /// # 返回值
    /// 返回匹配账号
    ///
    /// # 错误
    /// 当账号不存在或类型不匹配时返回错误。
    async fn load_account(
        &self,
        user_id: &str,
        account_kind: AccountKind,
        not_found_message: &str,
    ) -> Result<AccountCore> {
        account_of_kind(
            self.db.accounts().find_by_id(user_id).await?,
            account_kind,
            not_found_message,
        )
    }

    /// 计算账号权限集合。
    ///
    /// # 参数
    /// * `account` - 账号实体
    ///
    /// # 返回值
    /// 返回权限集合
    async fn account_permissions(&self, account: &AccountCore) -> Result<Vec<Permission>> {
        self.rbac
            .permissions(account.kind, account.base.id.as_str())
            .await
    }

    /// 构建统一账号资料响应。
    ///
    /// # 参数
    /// * `account` - 账号实体
    /// * `role_ids` - 角色ID集合
    /// * `permissions` - 权限集合
    /// * `avatar` - 已规范化的头像
    ///
    /// # 返回值
    /// 返回统一账号资料响应
    ///
    fn build_profile(
        account: AccountCore,
        role_ids: Vec<String>,
        permissions: Vec<Permission>,
        avatar: Option<String>,
    ) -> AccountProfile {
        let AccountCore {
            base,
            secret,
            name,
            phone,
            kind,
            ..
        } = account;
        let subject = iam::subject(kind, base.id.as_str());

        AccountProfile {
            user_id: base.id,
            account: secret.into_account(),
            name,
            email: None,
            phone,
            avatar,
            subject,
            role_ids,
            permissions,
            account_kind: kind,
            store_id: None,
        }
    }

    /// 归一化头像地址。
    ///
    /// # 参数
    /// * `avatar` - 原始头像地址
    ///
    /// # 返回值
    /// 返回有效头像地址；空值返回 `None`
    fn normalized_avatar(avatar: Option<&str>) -> Option<String> {
        avatar
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
    }
}

#[cfg(test)]
mod tests {
    use super::AccountProfileService;
    use entities::{
        AccountCore, AccountCoreData, AccountKind, AccountStatus, LoginAccount, Permission, Secret,
    };

    #[test]
    fn build_profile_should_keep_admin_identity_and_phone() {
        let account = AccountCore::new(
            "account-1".to_string(),
            AccountCoreData {
                secret: Secret::new(LoginAccount::new("admin01").unwrap(), "password01").unwrap(),
                name: "管理员".to_string(),
                kind: AccountKind::Admin,
                status: AccountStatus::Active,
                email: Some("admin@example.com".to_string()),
                phone: Some("13500000000".to_string()),
                avatar: None,
            },
        )
        .unwrap();

        let profile = AccountProfileService::build_profile(
            account,
            vec!["role-a".to_string()],
            vec![Permission::parse("admin:read").unwrap()],
            None,
        );

        assert_eq!(profile.email, None);
        assert_eq!(profile.phone.as_deref(), Some("13500000000"));
        assert_eq!(profile.subject, "user:admin:account-1");
    }

    #[test]
    fn normalized_avatar_should_trim_and_filter_empty_values() {
        let avatar = AccountProfileService::normalized_avatar(Some(" https://example.com/a.png "));
        assert_eq!(avatar.as_deref(), Some("https://example.com/a.png"));

        let empty = AccountProfileService::normalized_avatar(Some("   "));
        assert!(empty.is_none());
    }
}
