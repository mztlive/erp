use std::collections::BTreeSet;

use erp_core::AccountKind;
use mongodb::Database;
use persistence_core::NoTransaction;
use serde::Serialize;

use crate::entity::account_core::AccountCore;
use crate::entity::rbac::Permission;
use crate::entity::role_workbench::RoleWorkbench;
use crate::error::Result;
use crate::service::account_support::account_of_kind;
use crate::service::iam::SharedRbacService;
use crate::{AccessControlExt, RoleRepositoryExt};

/// 当前后台账号的资料和权限摘要，供客户端刷新会话展示。
#[derive(Debug, Serialize)]
pub struct AccountProfile {
    /// 当前账号 ID，对外字段名为 `userid`。
    #[serde(rename = "userid")]
    pub user_id: String,
    /// 本次资料读取校验通过的 Casbin 策略版本。
    pub policy_version: u64,
    /// 读取到的组织版本，尚无版本记录时为 `0`。
    pub organization_version: u64,
    /// 登录账号。
    pub account: String,
    /// 显示名称。
    pub name: String,
    /// 已登记的联系电话。
    pub phone: Option<String>,
    /// 去除首尾空白后的头像地址，缺失或全空白时为 `None`。
    pub avatar: Option<String>,
    /// 由账号类型与 ID 组成的 Casbin 主体标识。
    pub subject: String,
    /// 当前直接绑定的 Casbin 角色 ID。
    pub role_ids: Vec<String>,
    /// 根据启用角色与权限计算出的可用工作台。
    pub workbenches: BTreeSet<RoleWorkbench>,
    /// 根据 Casbin 策略读取的角色继承权限，用于功能展示。
    pub permissions: Vec<Permission>,
    /// 当前账号的身份类型；本用例仅交付有效后台账号的资料。
    pub account_kind: AccountKind,
}

/// 为已认证的后台账号提供本人资料、工作台和权限信息。
pub struct AccountProfileService {
    // 身份数据库，用于读取账号、启用角色和组织版本。
    db: Database,
    // 共享 Casbin 角色授权服务，用于读取绑定、权限和策略版本。
    rbac: SharedRbacService,
}

impl AccountProfileService {
    /// 组装当前账号资料查询时接入身份数据库和共享 Casbin 角色授权服务，构造阶段只保存依赖。
    pub fn new(db: Database, rbac: SharedRbacService) -> Self {
        Self { db, rbac }
    }

    /// 凭已认证的账号 ID 和类型读取本人资料、角色、权限、可用工作台及策略和组织版本；交付前再次比较策略版本。
    ///
    /// 账号缺失或类型不符返回 `NotFound`，已失效返回 `Forbidden`，策略版本变化返回 `ConflictError`；读取失败沿用原错误。
    pub async fn account_profile(&self, user_id: &str, account_kind: AccountKind) -> Result<AccountProfile> {
        let policy_version = self.rbac.current_policy_revision().await?;
        let account = account_of_kind(
            self.db.accounts().find_by_id(user_id, &mut NoTransaction).await?,
            account_kind,
            "管理员不存在",
        )?;

        let role_ids = self.rbac.role_ids(account.kind, account.base.id.as_str()).await?;
        let permissions = self.rbac.permissions(account.kind, account.base.id.as_str()).await?;
        let roles = self.db.roles().enabled_roles(&role_ids, &mut NoTransaction).await?;
        let workbenches = RoleWorkbench::available(&roles, permissions.as_slice());
        let avatar = Self::normalized_avatar(account.avatar.as_deref());
        if !account.is_active_backoffice() {
            return Err(crate::Error::Forbidden("账号已失效".into()));
        }
        let organization_version = crate::repository::OrganizationRepository::new(&self.db)
            .revision(&mut NoTransaction)
            .await?
            .map_or(0, |row| row.revision);

        if self.rbac.current_policy_revision().await? != policy_version {
            return Err(crate::Error::ConflictError("DATA_SCOPE_CHANGED：权限已变化，请刷新".into()));
        }

        let mut profile = Self::build_profile(
            account,
            role_ids.into_iter().map(|id| id.to_string()).collect(),
            permissions.into_vec(),
            avatar,
        );
        profile.workbenches = workbenches;
        profile.policy_version = policy_version;
        profile.organization_version = organization_version;
        Ok(profile)
    }

    // 交付前投影账号资料、角色和权限，并生成 Casbin 主体标识；头像已规范化，工作台和版本由调用方填入。
    fn build_profile(
        account: AccountCore,
        role_ids: Vec<String>,
        permissions: Vec<Permission>,
        avatar: Option<String>,
    ) -> AccountProfile {
        let AccountCore { base, secret, name, phone, kind, .. } = account;
        let subject = crate::subject(kind, base.id.as_str());
        AccountProfile {
            user_id: base.id,
            policy_version: 0,
            organization_version: 0,
            account: secret.into_account(),
            name,
            phone,
            avatar,
            subject,
            role_ids,
            workbenches: BTreeSet::new(),
            permissions,
            account_kind: kind,
        }
    }

    // 组装资料前裁去头像地址首尾空白，缺失或全空白时返回 `None`。
    fn normalized_avatar(avatar: Option<&str>) -> Option<String> {
        avatar.map(|value| value.trim().to_string()).filter(|value| !value.is_empty())
    }
}

#[cfg(test)]
mod tests {
    use erp_core::AccountKind;

    use super::AccountProfileService;
    use crate::entity::{AccountCore, AccountCoreData, AccountStatus, LoginAccount, Permission, Secret};

    // 验证资料投影保留联系电话和后台主体标识，序列化省略邮箱及门店字段。
    #[test]
    fn build_profile_should_keep_admin_identity_and_phone() {
        let mut account = AccountCore::new(
            "account-1".to_string(),
            AccountCoreData {
                secret: Secret::new(LoginAccount::new("admin01").unwrap(), "password01").unwrap(),
                name: "管理员".to_string(),
                kind: AccountKind::Admin,
                status: AccountStatus::Active,
            },
        )
        .unwrap();

        account.phone = Some("13500000000".into());
        account.email = Some("admin@example.com".into());
        let profile = AccountProfileService::build_profile(
            account,
            vec!["role-a".to_string()],
            vec![Permission::parse("admin:read").unwrap()],
            None,
        );

        let json = serde_json::to_value(&profile).unwrap();
        assert!(json.get("email").is_none());
        assert!(json.get("store_id").is_none());
        assert_eq!(profile.phone.as_deref(), Some("13500000000"));
        assert_eq!(profile.subject, "user:admin:account-1");
    }

    // 验证头像地址去除首尾空白，全空白地址映射为 `None`。
    #[test]
    fn normalized_avatar_should_trim_and_filter_empty_values() {
        let avatar = AccountProfileService::normalized_avatar(Some(" https://example.com/a.png "));
        assert_eq!(avatar.as_deref(), Some("https://example.com/a.png"));

        let empty = AccountProfileService::normalized_avatar(Some("   "));
        assert!(empty.is_none());
    }
}
