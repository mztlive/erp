use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use application_core::AuditActor;
use erp_core::AccountKind;
use erp_permissions::admin;
use mongodb::Database;
use persistence_core::{Executor, NoTransaction};
use validator::Validate;

use super::dto::{
    AdminItem, CreateAdminParams, InitializeSuperAdminParams, UpdateAdminParams, UpdateAdminRoleParams,
};
use super::scope::seed_company;
use crate::entity::account_core::{
    AccountCore, AccountCoreData, AccountCoreUpdate, AccountStatus, ExistingSuperAdminState,
};
use crate::entity::auth::LoginAccount;
use crate::entity::rbac::RoleIdSet;
use crate::error::Result;
use crate::ports::PreparedResourceAudit;
use crate::repository::prelude::*;
use crate::service::account_support::{account_of_kind, apply_account_update, ensure_account_available};
use crate::service::auth::password;
use crate::service::iam::management::ManagementAccess;
use crate::service::iam::{AuthorizedAccountManagement, AuthorizedRoleGrant, SharedRbacService};
use crate::{AccessControlExt, AuthorizationService};

/// 后台管理员账号用例，负责列表、账号维护、角色分配及超级管理员初始化。
pub struct AdminService {
    /// 身份数据库，用于读取账号及开启本域事务。
    pub(super) db: Database,
    /// 共享 Casbin 角色授权服务，用于角色取证和策略写事务。
    pub(super) rbac: SharedRbacService,
}

/// 运维初始化完成后的账号标识及变更结果。
#[derive(Debug, Clone)]
pub struct InitializeSuperAdminResult {
    /// 超级管理员账号 ID。
    pub admin_id: String,
    /// 规范化后的登录账号。
    pub account: String,
    /// 本次是否新建账号。
    pub created: bool,
    /// 是否恢复了原先不可登录或已软删除的账号。
    pub reactivated: bool,
    /// 本次是否为新账号建立或为原先缺少绑定的账号补齐 `role-root`。
    pub root_role_bound: bool,
}

// 区分超级管理员是首次创建还是通过运维命令修复已有账号。
enum SuperAdminInitialization {
    Created(
        // 已初始化根角色绑定和公司人员范围的新账号。
        AccountCore,
    ),
    Existing {
        // 完成资料更新和角色绑定后的账号。
        account: AccountCore,
        // 修复前是否不可登录或已软删除。
        reactivated: bool,
        // 修复前是否缺少 `role-root` 绑定。
        root_role_bound: bool,
    },
}

// 管理员资料更新的事务外准备结果，供写事务保存资料及可选角色。
struct AdminUpdateContext {
    // 已应用领域更新的管理员账号，变更的密码已哈希。
    account: AccountCore,
    // 目标管理资格对应的策略版本与可选的完整角色授予。
    authorization: AuthorizedAccountManagement,
}

impl SuperAdminInitialization {
    // 构造运维回执前拆出账号和新建、恢复启用、补绑根角色三个标记。
    fn into_parts(self) -> (AccountCore, bool, bool, bool) {
        match self {
            Self::Created(account) => (account, true, false, true),
            Self::Existing { account, reactivated, root_role_bound } => {
                (account, false, reactivated, root_role_bound)
            },
        }
    }
}

impl AdminService {
    /// 装配管理端或运维命令时接入身份数据库和共享 Casbin 角色授权服务，构造阶段只保存依赖。
    pub fn new(db: Database, rbac: SharedRbacService) -> Self {
        Self { db, rbac }
    }

    /// 管理端新增账号时先预检入口，再检查账号是否被占用、校验密码并确认可授予的角色；密码哈希在事务外生成。
    /// 写事务重验入口、授权策略版本及角色有效性，原子保存账号、Casbin 绑定和审计。
    ///
    /// 账号已占用、指定角色无效、权限不足或密码校验不通过时拒绝创建。
    pub async fn create_admin(
        &self,
        params: CreateAdminParams,
        actor: AuditActor,
        authorization: Arc<AuthorizationService>,
    ) -> Result<()> {
        let access = ManagementAccess::new(authorization, actor.clone(), &admin::ACCOUNT_CREATE_ADMIN);
        access.preflight(&self.db).await?;

        params.validate()?;
        let account = LoginAccount::new(params.account)?;
        ensure_account_available(&self.db, &account, None, &mut NoTransaction).await?;

        let role_ids = RoleIdSet::parse_non_empty(params.role_ids)?;
        let secret = password::hash_secret(account, params.password).await?;
        let id = id_generator::next_id();
        let data = AccountCoreData::admin(secret, params.name);
        let account = AccountCore::new(id, data)?;

        let grant = self.rbac.authorize_role_assignment(&actor, role_ids).await?;
        let audit =
            self.rbac.prepare_resource_log(actor, "admin.create", "admin", account.base.id.clone())?;
        self.create_account_with_roles(account, grant, audit, access).await?;
        Ok(())
    }

    /// 供运维 `init-admin` 确保内建根角色存在，并新建或修复超级管理员。
    /// 修复时更新名称和密码、恢复启用并将角色集设为 `role-root`。回执标明新建、恢复和补绑。
    /// 输入非法、同名账号类型不符、账号冲突或持久化失败时返回错误。
    pub async fn initialize_super_admin(
        &self,
        params: InitializeSuperAdminParams,
    ) -> Result<InitializeSuperAdminResult> {
        let (account, password, name) = params.into_validated_parts()?;

        crate::service::iam::ensure_root_role(&self.rbac).await?;
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

    /// 管理员列表在同一事务读取账号与 Casbin 直接角色绑定，返回前用原执行器再验列表入口。
    /// 授权和读取均通过后才交付完整列表。
    pub async fn admin_list(
        &self,
        actor: &AuditActor,
        authorization: Arc<AuthorizationService>,
    ) -> Result<Vec<AdminItem>> {
        let db = self.db.clone();
        let rbac = self.rbac.clone();
        ManagementAccess::new(authorization, actor.clone(), &admin::ACCOUNT_LIST_ADMINS)
            .verified_read_tx(&self.db, move |executor| {
                Box::pin(async move {
                    let admins = db.accounts().list_by_kind(AccountKind::Admin, executor).await?;
                    let account_ids =
                        admins.iter().map(|account| account.base.id.clone()).collect::<Vec<_>>();
                    let mut role_ids = rbac.account_roles(AccountKind::Admin, &account_ids, executor).await?;

                    Ok(admins
                        .into_iter()
                        .map(|account| {
                            let roles = role_ids.remove(&account.base.id).unwrap_or_default();
                            AdminItem::from_account(
                                account,
                                roles.into_iter().map(|id| id.to_string()).collect(),
                            )
                        })
                        .collect())
                })
            })
            .await
    }

    /// 管理端更新管理员姓名、密码、状态或角色时，先检查入口、目标管理资格及角色授予上限。
    /// 写事务重验入口和授权策略版本，保存资料、可选的完整角色替换及审计。
    ///
    /// 找不到目标、角色无效或试图停用当前登录账号时拒绝。
    pub async fn update_admin(
        &self,
        params: UpdateAdminParams,
        actor: AuditActor,
        authorization: Arc<AuthorizationService>,
    ) -> Result<()> {
        let access = ManagementAccess::new(authorization, actor.clone(), &admin::ACCOUNT_UPDATE_ADMIN);
        access.preflight(&self.db).await?;

        params.validate()?;
        let AdminUpdateContext { mut account, authorization } =
            self.prepare_admin_update(params, &actor).await?;
        let policy_revision = authorization.policy_revision();
        let grant = authorization.into_role_grant();
        let account_id = account.base.id.clone();
        let audit = self.rbac.prepare_resource_log(actor, "admin.update", "admin", account_id.clone())?;
        let db = self.db.clone();
        let rbac = self.rbac.clone();
        self.run_admin_account_write(policy_revision, audit, access, move |executor| {
            Box::pin(async move {
                db.accounts().update(&mut account, executor).await?;
                if let Some(grant) = grant {
                    rbac.assign_roles(AccountKind::Admin, &account_id, grant, executor).await?;
                }
                Ok(())
            })
        })
        .await
    }

    // 写事务前校验目标管理员及可选的完整角色集，应用资料变更并哈希新密码；写入时仍须重验管理授权。
    async fn prepare_admin_update(
        &self,
        params: UpdateAdminParams,
        actor: &AuditActor,
    ) -> Result<AdminUpdateContext> {
        let UpdateAdminParams { id, name, password, role_ids, status } = params;
        if status.as_ref().is_some_and(|status| !status.is_active()) && id == actor.id() {
            return Err(crate::error::Error::Forbidden("不能停用当前登录账号".to_string()));
        }
        let mut account = account_of_kind(
            self.db.accounts().find_by_id(&id, &mut NoTransaction).await?,
            AccountKind::Admin,
            "管理员不存在",
        )?;

        let role_ids = role_ids.map(RoleIdSet::parse_non_empty).transpose()?;
        let authorization = self
            .rbac
            .authorize_target_management(actor, AccountKind::Admin, account.base.id.as_str(), role_ids)
            .await?;

        account = apply_account_update(account, AccountCoreUpdate { name, password, status }).await?;
        Ok(AdminUpdateContext { account, authorization })
    }

    /// 管理端仅更换角色时，校验目标及新角色的授予资格后，在写事务替换决定实际权限的 Casbin 角色集并记录审计。
    /// 目标不存在、角色无效或写事务复验失败时拒绝。
    pub async fn update_admin_role(
        &self,
        params: UpdateAdminRoleParams,
        actor: AuditActor,
        authorization: Arc<AuthorizationService>,
    ) -> Result<()> {
        let access = ManagementAccess::new(authorization, actor.clone(), &admin::ACCOUNT_UPDATE_ADMIN_ROLE);
        access.preflight(&self.db).await?;

        params.validate()?;
        let account = account_of_kind(
            self.db.accounts().find_by_id(&params.id, &mut NoTransaction).await?,
            AccountKind::Admin,
            "管理员不存在",
        )?;

        let role_ids = RoleIdSet::parse_non_empty(params.role_ids)?;
        let authorization = self
            .rbac
            .authorize_target_management(&actor, AccountKind::Admin, account.base.id.as_str(), Some(role_ids))
            .await?;
        let grant = authorization
            .into_role_grant()
            .ok_or_else(|| crate::error::Error::Internal("管理员角色授权上下文缺少角色集合".to_string()))?;

        let policy_revision = grant.policy_revision();
        let account_id = account.base.id;
        let audit =
            self.rbac.prepare_resource_log(actor, "admin.role.update", "admin", account_id.clone())?;
        let rbac = self.rbac.clone();
        self.run_admin_account_write(policy_revision, audit, access, move |executor| {
            Box::pin(async move {
                rbac.assign_roles(AccountKind::Admin, &account_id, grant, executor).await?;
                Ok(())
            })
        })
        .await
    }

    /// 管理端删除管理员时先校验入口和目标管理资格，再于写事务软删除账号、清除 Casbin 绑定并记录审计。
    /// 目标不存在、资格不符或持久化失败时返回错误。
    pub async fn delete_admin(
        &self,
        id: String,
        actor: AuditActor,
        authorization: Arc<AuthorizationService>,
    ) -> Result<()> {
        let access = ManagementAccess::new(authorization, actor.clone(), &admin::ACCOUNT_DELETE_ADMIN);
        access.preflight(&self.db).await?;

        let account = account_of_kind(
            self.db.accounts().find_by_id(&id, &mut NoTransaction).await?,
            AccountKind::Admin,
            "管理员不存在",
        )?;

        let authorization = self
            .rbac
            .authorize_target_management(&actor, AccountKind::Admin, account.base.id.as_str(), None)
            .await?;

        let audit = self.rbac.prepare_resource_log(actor, "admin.delete", "admin", id)?;
        self.delete_account_with_roles(account, audit, authorization.policy_revision(), access).await
    }

    // 管理端完成新账号和完整角色授予的准备后，在同一事务保存账号、Casbin 绑定及审计；写入或提交失败时返回错误。
    async fn create_account_with_roles(
        &self,
        account: AccountCore,
        grant: AuthorizedRoleGrant,
        audit: PreparedResourceAudit,
        access: ManagementAccess,
    ) -> Result<()> {
        let db = self.db.clone();
        let rbac = self.rbac.clone();
        let account_id = account.base.id.clone();
        let policy_revision = grant.policy_revision();
        self.run_admin_account_write(policy_revision, audit, access, move |executor| {
            Box::pin(async move {
                db.accounts().create(&account, executor).await?;
                rbac.assign_roles(AccountKind::Admin, &account_id, grant, executor).await?;
                Ok(())
            })
        })
        .await
    }

    // 首次创建超级管理员时，在同一系统策略事务保存账号、根角色绑定和公司人员范围。
    async fn create_system_account_with_roles(
        &self,
        account: AccountCore,
        role_ids: RoleIdSet,
    ) -> Result<AccountCore> {
        let db = self.db.clone();
        let rbac = self.rbac.clone();
        let account_id = account.base.id.clone();
        self.rbac
            .run_system_policy_transaction(move |executor| {
                Box::pin(async move {
                    db.accounts().create(&account, executor).await?;
                    rbac.assign_system_roles(AccountKind::Admin, &account_id, role_ids, executor).await?;
                    seed_company(&db, &account_id, executor).await?;
                    Ok::<AccountCore, crate::error::Error>(account)
                })
            })
            .await
    }

    // 运维修复已有管理员时，在同一系统策略事务按需恢复软删除记录、保存新资料，并将 Casbin 角色集替换为 `role-root`。
    async fn repair_system_account(
        &self,
        mut account: AccountCore,
        state: ExistingSuperAdminState,
    ) -> Result<AccountCore> {
        let db = self.db.clone();
        let rbac = self.rbac.clone();
        let account_id = account.base.id.clone();
        self.rbac
            .run_system_policy_transaction(move |executor| {
                Box::pin(async move {
                    if matches!(state, ExistingSuperAdminState::Deleted) {
                        db.accounts().restore(&mut account, executor).await?;
                    }
                    db.accounts().update(&mut account, executor).await?;
                    rbac.assign_system_roles(
                        AccountKind::Admin,
                        &account_id,
                        RoleIdSet::parse([crate::ROOT_ROLE_ID])?,
                        executor,
                    )
                    .await?;
                    Ok::<AccountCore, crate::error::Error>(account)
                })
            })
            .await
    }

    // 删除入口准备好审计后，以目标授权时的策略版本开启写事务，原子软删除账号、清除全部 Casbin 绑定并写审计。
    async fn delete_account_with_roles(
        &self,
        mut account: AccountCore,
        audit: PreparedResourceAudit,
        policy_revision: u64,
        access: ManagementAccess,
    ) -> Result<()> {
        let db = self.db.clone();
        let rbac = self.rbac.clone();
        let account_id = account.base.id.clone();
        self.run_admin_account_write(policy_revision, audit, access, move |executor| {
            Box::pin(async move {
                db.accounts().soft_delete(&mut account, executor).await?;
                rbac.clear_roles(AccountKind::Admin, &account_id, executor).await?;
                Ok(())
            })
        })
        .await
    }

    // 管理端账号写入统一在策略事务中重验入口和授权快照版本，并原子提交业务修改与审计。
    // 提交后刷新本地 Casbin 策略；失败时记录错误，后续授权须先刷新成功。系统初始化使用独立的系统策略事务入口。
    async fn run_admin_account_write<T, F>(
        &self,
        policy_revision: u64,
        audit: PreparedResourceAudit,
        access: ManagementAccess,
        write: F,
    ) -> Result<T>
    where
        T: Send + 'static,
        F: for<'a> FnOnce(&'a mut dyn Executor) -> Pin<Box<dyn Future<Output = Result<T>> + Send + 'a>>
            + Send
            + 'static,
    {
        self.rbac
            .run_authorized_audited_policy_transaction(policy_revision, audit, move |executor| {
                Box::pin(async move {
                    access.require(executor).await?;
                    write(executor).await
                })
            })
            .await
    }

    // 运维初始化时按登录名查找账号（含软删除），决定新建还是修复，以取得启用且绑定 `role-root` 的管理员。
    async fn ensure_super_admin(
        &self,
        account: LoginAccount,
        password: String,
        name: String,
    ) -> Result<SuperAdminInitialization> {
        if let Some(current) =
            self.db.accounts().find_by_account_including_deleted(account.as_str(), &mut NoTransaction).await?
        {
            return self.ensure_existing_super_admin(current, password, name).await;
        }

        let admin = self.create_super_admin(account, password, name).await?;
        Ok(SuperAdminInitialization::Created(admin))
    }

    // 运维初始化命中已有账号时，只修复系统管理员；先读取原状态和根角色绑定、准备新名称和密码，再原子写入。
    // 按修复前状态生成结果标记；同名非管理员返回业务错误，角色读取、密码处理或写库失败沿用原错误。
    async fn ensure_existing_super_admin(
        &self,
        current: AccountCore,
        password: String,
        name: String,
    ) -> Result<SuperAdminInitialization> {
        if !current.is_kind(AccountKind::Admin) {
            return Err(crate::error::Error::BusinessLogicError(
                "账号存在但不属于系统管理员，无法初始化为超级管理员".to_string(),
            ));
        }

        let state =
            ExistingSuperAdminState::from_state(current.base.is_deleted(), current.status.is_active());
        let role_ids = self.rbac.role_ids(AccountKind::Admin, current.base.id.as_str()).await?;
        let has_root_role = role_ids.iter().any(|role_id| role_id.as_str() == crate::ROOT_ROLE_ID);

        let current = apply_account_update(
            current,
            AccountCoreUpdate {
                name: Some(name),
                password: Some(password),
                status: Some(AccountStatus::Active),
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

    /// 启动引导或运维初始化确认账号缺失后，生成密码哈希并创建启用的管理员，同时绑定 `role-root` 和初始化公司人员范围。
    /// 校验、哈希或持久化失败时返回错误。
    pub(super) async fn create_super_admin(
        &self,
        account: LoginAccount,
        password: String,
        name: String,
    ) -> Result<AccountCore> {
        let secret = password::hash_secret(account, password).await?;
        let admin = AccountCore::new(id_generator::next_id(), AccountCoreData::admin(secret, name))?;
        self.create_system_account_with_roles(admin, RoleIdSet::parse([crate::ROOT_ROLE_ID])?).await
    }
}

#[cfg(test)]
mod tests {
    use crate::entity::account_core::ExistingSuperAdminState;

    // 验证初始化状态分类覆盖启用、停用和软删除，且软删除优先于启用标记。
    #[test]
    fn existing_super_admin_state_covers_active_inactive_and_deleted_accounts() {
        assert_eq!(ExistingSuperAdminState::from_state(false, true), ExistingSuperAdminState::Active);
        assert_eq!(ExistingSuperAdminState::from_state(false, false), ExistingSuperAdminState::Inactive);
        assert_eq!(ExistingSuperAdminState::from_state(true, true), ExistingSuperAdminState::Deleted);
        assert_eq!(ExistingSuperAdminState::from_state(true, false), ExistingSuperAdminState::Deleted);
    }

    // 验证修复停用或软删除账号均计为恢复启用，原本启用的账号不计入。
    #[test]
    fn inactive_and_deleted_super_admins_are_reported_as_reactivated() {
        assert!(!ExistingSuperAdminState::Active.reactivates());
        assert!(ExistingSuperAdminState::Inactive.reactivates());
        assert!(ExistingSuperAdminState::Deleted.reactivates());
    }
}
