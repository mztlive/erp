use database::{AccessControlExt, NoTransaction};
use entities::{
    AccountCore, AccountCoreData, AccountCoreUpdate, AccountKind, AccountStatus, AuditLog, LoginAccount,
    RoleIdSet,
};
use mongodb::Database;
use validator::Validate;

use super::dto::{
    AdminItem, CreateAdminParams, InitializeSuperAdminParams, UpdateAdminParams, UpdateAdminRoleParams,
};
use crate::account_support::{account_of_kind, apply_account_update, ensure_account_available};
use crate::audit::AuditActor;
use crate::auth::password;
use crate::errors::Result;
use crate::iam::{self, AuthorizedAccountManagement, AuthorizedRoleGrant, SharedRbacService};

/// 管理员服务
///
/// 提供管理员账号的创建、更新、删除及角色分配相关业务逻辑。
pub struct AdminService {
    db: Database,
    rbac: SharedRbacService,
}

/// 超级管理员初始化结果。
#[derive(Debug, Clone)]
pub struct InitializeSuperAdminResult {
    /// 超级管理员账号ID。
    pub admin_id: String,
    /// 超级管理员账号名。
    pub account: String,
    /// 是否新建了管理员账号。
    pub created: bool,
    /// 是否将已有账号从停用状态恢复为启用。
    pub reactivated: bool,
    /// 是否新增了 `role-root` 绑定。
    pub root_role_bound: bool,
}

/// 已存在超级管理员的账号状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExistingSuperAdminState {
    Active,
    Inactive,
    Deleted,
}

impl ExistingSuperAdminState {
    /// 根据软删除与登录状态确定恢复语义。
    fn from_state(is_deleted: bool, is_active: bool) -> Self {
        if is_deleted {
            return Self::Deleted;
        }
        if is_active {
            Self::Active
        } else {
            Self::Inactive
        }
    }

    /// 返回初始化是否恢复了不可登录或已删除账号。
    fn reactivates(self) -> bool {
        !matches!(self, Self::Active)
    }
}

/// 超级管理员初始化后的合法状态。
enum SuperAdminInitialization {
    Created(AccountCore),
    Existing {
        account: AccountCore,
        reactivated: bool,
        root_role_bound: bool,
    },
}

/// 已应用账号领域更新并规范化角色集合的管理员更新上下文。
struct AdminUpdateContext {
    account: AccountCore,
    authorization: AuthorizedAccountManagement,
}

impl SuperAdminInitialization {
    /// 将内部状态转换为现有响应合同所需的字段。
    fn into_parts(self) -> (AccountCore, bool, bool, bool) {
        match self {
            Self::Created(account) => (account, true, false, true),
            Self::Existing {
                account,
                reactivated,
                root_role_bound,
            } => (account, false, reactivated, root_role_bound),
        }
    }
}

impl AdminService {
    /// 使用共享 RBAC 服务创建管理员服务实例。
    ///
    /// # 参数
    /// * `db` - 数据库实例
    /// * `rbac` - 共享 Casbin RBAC 服务
    ///
    /// # 返回值
    /// 返回管理员服务实例。
    pub fn new(db: Database, rbac: SharedRbacService) -> Self {
        Self { db, rbac }
    }

    /// 创建管理员账号。
    ///
    /// 会校验账号唯一性以及角色存在性，然后为管理员生成安全凭证并持久化。
    ///
    /// # 参数
    /// * `params` - 创建管理员所需参数
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 错误
    /// * `ConflictError` - 账号已存在
    /// * `BusinessLogicError` - 角色不存在或密码策略等业务校验失败
    pub async fn create_admin(&self, params: CreateAdminParams, actor: AuditActor) -> Result<()> {
        params.validate()?;
        let account = LoginAccount::new(params.account)?;
        ensure_account_available(&self.db, &account, None, &mut NoTransaction).await?;

        let role_ids = RoleIdSet::parse_non_empty(params.role_ids)?.to_strings();
        let secret = password::hash_secret(account, params.password).await?;
        let id = id_generator::next_id();
        let data = AccountCoreData {
            secret,
            name: params.name,
            kind: AccountKind::Admin,
            status: AccountStatus::Active,
            email: None,
            phone: None,
            avatar: None,
        };
        let account = AccountCore::new(id, data)?;
        let grant = self.rbac.authorize_role_assignment(&actor, role_ids).await?;
        let audit = actor.resource_log("admin.create", "admin", account.base.id.clone())?;
        self.create_account_with_roles(account, grant, audit).await?;

        Ok(())
    }

    /// 初始化系统超级管理员账号。
    ///
    /// # 参数
    /// * `params` - 初始化参数，账号/密码/名称均必填
    ///
    /// # 返回值
    /// 返回初始化执行结果
    ///
    /// # 错误
    /// * `BusinessLogicError` - 参数非法、账号冲突或账号域不合法
    /// * `RepositoryError` - 数据库读写失败
    pub async fn initialize_super_admin(
        &self,
        params: InitializeSuperAdminParams,
    ) -> Result<InitializeSuperAdminResult> {
        let (account, password, name) = params.into_validated_parts()?;
        iam::ensure_root_role(&self.rbac).await?;
        let initialization = self.ensure_super_admin(account.clone(), password, name).await?;
        let (admin, created, reactivated, root_role_bound) = initialization.into_parts();

        Ok(InitializeSuperAdminResult {
            admin_id: admin.base.id,
            account: account.into_string(),
            created,
            reactivated,
            root_role_bound,
        })
    }

    /// 获取管理员列表。
    ///
    /// # 返回值
    /// * `Ok(Vec<AdminItem>)` - 包含角色 ID 的管理员集合
    pub async fn admin_list(&self) -> Result<Vec<AdminItem>> {
        let admins: Vec<AccountCore> = self
            .db
            .accounts()
            .list_by_kind(AccountKind::Admin, &mut NoTransaction)
            .await?;
        let account_ids = admins
            .iter()
            .map(|account| account.base.id.clone())
            .collect::<Vec<_>>();
        let mut role_ids = self
            .rbac
            .role_ids_by_accounts(AccountKind::Admin, &account_ids)
            .await?;

        Ok(admins
            .into_iter()
            .map(|account| {
                let roles = role_ids.remove(&account.base.id).unwrap_or_default();
                AdminItem::from_account(account, roles)
            })
            .collect())
    }

    /// 更新管理员信息。
    ///
    /// 支持更新姓名、密码、角色（角色需存在）。
    ///
    /// # 参数
    /// * `params` - 更新参数（包含管理员ID与可选更新字段）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 错误
    /// * `NotFound` - 管理员不存在
    /// * `BusinessLogicError` - 指定角色不存在
    pub async fn update_admin(&self, params: UpdateAdminParams, actor: AuditActor) -> Result<()> {
        params.validate()?;
        let context = self.prepare_admin_update(params, &actor).await?;
        let resource_id = context.account.base.id.clone();
        let policy_revision = context.authorization.policy_revision();
        match context.authorization.into_role_grant() {
            Some(grant) => {
                let audit = actor.resource_log("admin.update", "admin", resource_id)?;
                self.update_account_with_roles(context.account, grant, audit)
                    .await?;
            }
            None => {
                let audit = actor.resource_log("admin.update", "admin", resource_id)?;
                self.update_account_only(context.account, audit, policy_revision)
                    .await?;
            }
        }
        Ok(())
    }

    /// 在事务前应用管理员账号更新并规范化可选角色集合。
    async fn prepare_admin_update(
        &self,
        params: UpdateAdminParams,
        actor: &AuditActor,
    ) -> Result<AdminUpdateContext> {
        let UpdateAdminParams {
            id,
            name,
            password,
            role_ids,
        } = params;
        let mut account = account_of_kind(
            self.db.accounts().find_by_id(&id, &mut NoTransaction).await?,
            AccountKind::Admin,
            "管理员不存在",
        )?;

        let role_ids = role_ids
            .map(RoleIdSet::parse_non_empty)
            .transpose()?
            .map(|role_ids| role_ids.to_strings());
        let authorization = self
            .rbac
            .authorize_target_management(actor, AccountKind::Admin, account.base.id.as_str(), role_ids)
            .await?;
        account = apply_account_update(
            account,
            AccountCoreUpdate {
                name,
                password,
                ..Default::default()
            },
        )
        .await?;
        Ok(AdminUpdateContext {
            account,
            authorization,
        })
    }

    /// 在单个事务中更新管理员账号与审计日志。
    async fn update_account_only(
        &self,
        mut account: AccountCore,
        audit: AuditLog,
        policy_revision: u64,
    ) -> Result<()> {
        let db = self.db.clone();
        self.rbac
            .run_authorized_audited_policy_transaction(policy_revision, audit, move |session| {
                Box::pin(async move {
                    db.accounts().update(&mut account, session).await?;
                    Ok::<(), crate::errors::Error>(())
                })
            })
            .await
    }

    /// 单独更新管理员角色。
    ///
    /// # 参数
    /// * `params` - 包含管理员ID与新角色ID集合
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 错误
    /// * `NotFound` - 管理员不存在
    /// * `BusinessLogicError` - 角色不存在
    pub async fn update_admin_role(&self, params: UpdateAdminRoleParams, actor: AuditActor) -> Result<()> {
        params.validate()?;
        let account = account_of_kind(
            self.db
                .accounts()
                .find_by_id(&params.id, &mut NoTransaction)
                .await?,
            AccountKind::Admin,
            "管理员不存在",
        )?;

        let role_ids = RoleIdSet::parse_non_empty(params.role_ids)?.to_strings();
        let authorization = self
            .rbac
            .authorize_target_management(
                &actor,
                AccountKind::Admin,
                account.base.id.as_str(),
                Some(role_ids),
            )
            .await?;
        let grant = authorization
            .into_role_grant()
            .ok_or_else(|| crate::errors::Error::Internal("管理员角色授权上下文缺少角色集合".to_string()))?;
        let policy_revision = grant.policy_revision();
        let account_id = account.base.id;
        let audit = actor.resource_log("admin.role.update", "admin", account_id.clone())?;
        let rbac = self.rbac.clone();
        self.rbac
            .run_authorized_audited_policy_transaction(policy_revision, audit, move |session| {
                Box::pin(async move {
                    rbac.assign_roles(AccountKind::Admin, &account_id, grant, session)
                        .await?;
                    Ok::<(), crate::errors::Error>(())
                })
            })
            .await
    }

    /// 删除管理员（软删除）。
    ///
    /// # 参数
    /// * `id` - 管理员ID
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 错误
    /// * `NotFound` - 管理员不存在
    pub async fn delete_admin(&self, id: String, actor: AuditActor) -> Result<()> {
        let account = account_of_kind(
            self.db.accounts().find_by_id(&id, &mut NoTransaction).await?,
            AccountKind::Admin,
            "管理员不存在",
        )?;

        let authorization = self
            .rbac
            .authorize_target_management(&actor, AccountKind::Admin, account.base.id.as_str(), None)
            .await?;
        let audit = actor.resource_log("admin.delete", "admin", id)?;
        self.delete_account_with_roles(account, audit, authorization.policy_revision())
            .await
    }

    /// 在同一事务中创建管理员账号并写入完整角色绑定。
    ///
    /// # 参数
    /// * `account` - 待创建管理员实体
    /// * `role_ids` - 完整角色 ID 集合
    /// * `audit` - 管理端调用的审计日志；仅系统初始化可传 `None`
    ///
    /// # 返回值
    /// 返回已持久化的管理员实体。
    ///
    /// # 错误
    /// 当账号、角色绑定、事务提交或 policy 刷新失败时返回错误。
    async fn create_account_with_roles(
        &self,
        account: AccountCore,
        grant: AuthorizedRoleGrant,
        audit: AuditLog,
    ) -> Result<AccountCore> {
        let db = self.db.clone();
        let rbac = self.rbac.clone();
        let account_id = account.base.id.clone();
        let policy_revision = grant.policy_revision();
        self.rbac
            .run_authorized_audited_policy_transaction(policy_revision, audit, move |session| {
                Box::pin(async move {
                    db.accounts().create(&account, session).await?;
                    rbac.assign_roles(AccountKind::Admin, &account_id, grant, session)
                        .await?;
                    Ok::<AccountCore, crate::errors::Error>(account)
                })
            })
            .await
    }

    /// 在同一事务中更新管理员账号并覆盖完整角色绑定。
    ///
    /// # 参数
    /// * `account` - 已应用领域更新的管理员实体
    /// * `role_ids` - 替换后的完整角色 ID 集合
    /// * `audit` - 管理端调用的审计日志；仅系统初始化可传 `None`
    ///
    /// # 返回值
    /// 返回已更新持久化元数据的管理员实体。
    ///
    /// # 错误
    /// 当账号、角色绑定、事务提交或 policy 刷新失败时返回错误。
    async fn update_account_with_roles(
        &self,
        mut account: AccountCore,
        grant: AuthorizedRoleGrant,
        audit: AuditLog,
    ) -> Result<AccountCore> {
        let db = self.db.clone();
        let rbac = self.rbac.clone();
        let account_id = account.base.id.clone();
        let policy_revision = grant.policy_revision();
        self.rbac
            .run_authorized_audited_policy_transaction(policy_revision, audit, move |session| {
                Box::pin(async move {
                    db.accounts().update(&mut account, session).await?;
                    rbac.assign_roles(AccountKind::Admin, &account_id, grant, session)
                        .await?;
                    Ok::<AccountCore, crate::errors::Error>(account)
                })
            })
            .await
    }

    /// 在系统初始化事务中创建管理员并绑定内建角色。
    async fn create_system_account_with_roles(
        &self,
        account: AccountCore,
        role_ids: Vec<String>,
    ) -> Result<AccountCore> {
        let db = self.db.clone();
        let rbac = self.rbac.clone();
        let account_id = account.base.id.clone();
        self.rbac
            .run_system_policy_transaction(move |session| {
                Box::pin(async move {
                    db.accounts().create(&account, session).await?;
                    rbac.assign_system_roles(AccountKind::Admin, &account_id, role_ids, session)
                        .await?;
                    Ok::<AccountCore, crate::errors::Error>(account)
                })
            })
            .await
    }

    /// 在系统初始化事务中更新或恢复管理员并绑定 root 角色。
    async fn repair_system_account(
        &self,
        mut account: AccountCore,
        state: ExistingSuperAdminState,
    ) -> Result<AccountCore> {
        let db = self.db.clone();
        let rbac = self.rbac.clone();
        let account_id = account.base.id.clone();
        self.rbac
            .run_system_policy_transaction(move |session| {
                Box::pin(async move {
                    if matches!(state, ExistingSuperAdminState::Deleted) {
                        db.accounts().restore(&mut account, session).await?;
                    }
                    db.accounts().update(&mut account, session).await?;
                    rbac.assign_system_roles(
                        AccountKind::Admin,
                        &account_id,
                        vec![iam::ROOT_ROLE_ID.to_string()],
                        session,
                    )
                    .await?;
                    Ok::<AccountCore, crate::errors::Error>(account)
                })
            })
            .await
    }

    /// 在同一事务中软删除管理员账号并清除全部角色绑定。
    ///
    /// # 参数
    /// * `account` - 待软删除管理员实体
    /// * `audit` - 已验证的管理操作审计日志
    ///
    /// # 返回值
    /// 账号和绑定均删除成功时返回 `Ok(())`。
    ///
    /// # 错误
    /// 当账号、角色绑定、事务提交或 policy 刷新失败时返回错误。
    async fn delete_account_with_roles(
        &self,
        mut account: AccountCore,
        audit: AuditLog,
        policy_revision: u64,
    ) -> Result<()> {
        let db = self.db.clone();
        let rbac = self.rbac.clone();
        let account_id = account.base.id.clone();
        self.rbac
            .run_authorized_audited_policy_transaction(policy_revision, audit, move |session| {
                Box::pin(async move {
                    db.accounts().soft_delete(&mut account, session).await?;
                    rbac.clear_roles(AccountKind::Admin, &account_id, session).await?;
                    Ok::<(), crate::errors::Error>(())
                })
            })
            .await
    }

    /// 确保超级管理员账号存在、启用并绑定 root 角色。
    async fn ensure_super_admin(
        &self,
        account: LoginAccount,
        password: String,
        name: String,
    ) -> Result<SuperAdminInitialization> {
        if let Some(current) = self
            .db
            .accounts()
            .find_by_account_including_deleted(account.as_str(), &mut NoTransaction)
            .await?
        {
            return self.ensure_existing_super_admin(current, password, name).await;
        }

        let admin = self.create_super_admin(account, password, name).await?;
        Ok(SuperAdminInitialization::Created(admin))
    }

    /// 校验并原子修复已存在的超级管理员账号。
    ///
    /// # 参数
    /// * `current` - 已存在账号
    ///
    /// # 返回值
    /// 返回修复后的账号与实际写入状态。
    ///
    /// # 错误
    /// 当账号不符合系统管理员约束时返回错误。
    async fn ensure_existing_super_admin(
        &self,
        current: AccountCore,
        password: String,
        name: String,
    ) -> Result<SuperAdminInitialization> {
        if !current.is_kind(AccountKind::Admin) {
            return Err(crate::errors::Error::BusinessLogicError(
                "账号存在但不属于系统管理员，无法初始化为超级管理员".to_string(),
            ));
        }
        let state =
            ExistingSuperAdminState::from_state(current.base.is_deleted(), current.status.is_active());
        let role_ids = self
            .rbac
            .role_ids(AccountKind::Admin, current.base.id.as_str())
            .await?;
        let has_root_role = role_ids.iter().any(|role_id| role_id == iam::ROOT_ROLE_ID);
        let current = apply_account_update(
            current,
            AccountCoreUpdate {
                name: Some(name),
                password: Some(password),
                status: Some(AccountStatus::Active),
                ..Default::default()
            },
        )
        .await?;
        let current = self.repair_system_account(current, state).await?;
        Ok(SuperAdminInitialization::Existing {
            account: current,
            reactivated: state.reactivates(),
            root_role_bound: !has_root_role,
        })
    }

    /// 创建系统超级管理员账号。
    ///
    /// # 参数
    /// * `account` - 超级管理员账号
    /// * `password` - 超级管理员密码
    /// * `name` - 超级管理员名称
    ///
    /// # 返回值
    /// 返回新建的管理员账号
    ///
    /// # 错误
    /// 当参数校验或持久化失败时返回错误。
    async fn create_super_admin(
        &self,
        account: LoginAccount,
        password: String,
        name: String,
    ) -> Result<AccountCore> {
        let secret = password::hash_secret(account, password).await?;
        let admin = AccountCore::new(
            id_generator::next_id(),
            AccountCoreData {
                secret,
                name,
                kind: AccountKind::Admin,
                status: AccountStatus::Active,
                email: None,
                phone: None,
                avatar: None,
            },
        )?;
        self.create_system_account_with_roles(admin, vec![iam::ROOT_ROLE_ID.to_string()])
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::ExistingSuperAdminState;

    #[test]
    fn existing_super_admin_state_covers_active_inactive_and_deleted_accounts() {
        assert_eq!(
            ExistingSuperAdminState::from_state(false, true),
            ExistingSuperAdminState::Active
        );
        assert_eq!(
            ExistingSuperAdminState::from_state(false, false),
            ExistingSuperAdminState::Inactive
        );
        assert_eq!(
            ExistingSuperAdminState::from_state(true, true),
            ExistingSuperAdminState::Deleted
        );
        assert_eq!(
            ExistingSuperAdminState::from_state(true, false),
            ExistingSuperAdminState::Deleted
        );
    }

    #[test]
    fn inactive_and_deleted_super_admins_are_reported_as_reactivated() {
        assert!(!ExistingSuperAdminState::Active.reactivates());
        assert!(ExistingSuperAdminState::Inactive.reactivates());
        assert!(ExistingSuperAdminState::Deleted.reactivates());
    }
}
