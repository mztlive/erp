use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use application_core::AuditActor;
use erp_core::AccountKind;
use erp_permissions::admin;
use mongodb::Database;
use persistence_core::{Executor, NoTransaction, Transactional};
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

/// 管理员服务
///
/// 提供管理员账号的创建、更新、删除及角色分配相关业务逻辑。
pub struct AdminService {
    pub(super) db: Database,
    pub(super) rbac: SharedRbacService,
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

/// 超级管理员初始化后的合法状态。
enum SuperAdminInitialization {
    Created(AccountCore),
    Existing { account: AccountCore, reactivated: bool, root_role_bound: bool },
}

/// 已应用账号领域更新并规范化角色集合的管理员更新上下文。
struct AdminUpdateContext {
    account: AccountCore,
    authorization: AuthorizedAccountManagement,
}

impl SuperAdminInitialization {
    // 将新建、恢复或补绑后的初始化状态拆为管理员账号及客户端需要的结果标记。
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
    /// 在应用装配时用身份库和共享 RBAC 服务构造管理员用例，不在构造时取证或写库。
    pub fn new(db: Database, rbac: SharedRbacService) -> Self {
        Self { db, rbac }
    }

    /// 通过管理员创建入口预检后，验证账号唯一性、密码和可授予角色，生成密码哈希。实际写事务重新检查入口与授予条件，并一并保存账号、角色绑定和审计。
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

    /// 供系统初始化流程准备内建根角色和系统管理员。结果区分新建账号、恢复启用和补齐根角色绑定；输入非法、账号冲突或持久化失败时返回错误。
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

    /// 供管理员管理页读取管理员和当前角色 ID。通过固定列表资格后在同一事务读取账号与角色绑定，返回前再次检查入口；授权或读取失败不返回部分列表。
    pub async fn admin_list(
        &self,
        actor: &AuditActor,
        authorization: Arc<AuthorizationService>,
    ) -> Result<Vec<AdminItem>> {
        let db = self.db.clone();
        let rbac = self.rbac.clone();
        let access = ManagementAccess::new(authorization, actor.clone(), &admin::ACCOUNT_LIST_ADMINS);
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    access.require(executor).await?;

                    let admins = db.accounts().list_by_kind(AccountKind::Admin, executor).await?;
                    let account_ids =
                        admins.iter().map(|account| account.base.id.clone()).collect::<Vec<_>>();
                    let mut role_ids = rbac.account_roles(AccountKind::Admin, &account_ids, executor).await?;

                    let items = admins
                        .into_iter()
                        .map(|account| {
                            let roles = role_ids.remove(&account.base.id).unwrap_or_default();
                            AdminItem::from_account(
                                account,
                                roles.into_iter().map(|id| id.to_string()).collect(),
                            )
                        })
                        .collect();

                    access.require(executor).await?;
                    Ok(items)
                })
            })
            .await
    }

    /// 管理端更新管理员姓名、密码、状态或角色时，先检查入口、目标管理资格及角色授予上限，再在写事务重验并保存。
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
        let context = self.prepare_admin_update(params, &actor).await?;
        let resource_id = context.account.base.id.clone();
        let policy_revision = context.authorization.policy_revision();

        match context.authorization.into_role_grant() {
            Some(grant) => {
                let audit = self.rbac.prepare_resource_log(actor, "admin.update", "admin", resource_id)?;
                self.update_account_with_roles(context.account, grant, audit, access).await?;
            },
            None => {
                let audit = self.rbac.prepare_resource_log(actor, "admin.update", "admin", resource_id)?;
                self.update_account_only(context.account, audit, policy_revision, access).await?;
            },
        }
        Ok(())
    }

    // 在写事务前加载并校验目标管理员、规范化可选角色，并准备资料变更；实际写入仍须重验管理授权。
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

    // 未变更角色时在同一个管理员写事务内更新账号资料并保存审计，不另开策略写事务。
    async fn update_account_only(
        &self,
        mut account: AccountCore,
        audit: PreparedResourceAudit,
        policy_revision: u64,
        access: ManagementAccess,
    ) -> Result<()> {
        let db = self.db.clone();
        self.run_admin_account_write(policy_revision, audit, access, move |executor| {
            Box::pin(async move {
                db.accounts().update(&mut account, executor).await?;
                Ok(())
            })
        })
        .await
    }

    /// 管理端仅换角色时替换目标管理员的完整角色集。写入前核对目标及新角色的授予资格；目标不存在或角色无效时拒绝。
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

    /// 通过管理员删除入口和目标管理资格后，在写事务内软删除管理员、清除其角色绑定并记录审计。目标不存在、资格或持久化校验失败均拒绝。
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

    // 在同一事务中创建管理员账号并写入完整角色绑定。
    // 账号、角色绑定和审计写入成功时返回成功。
    //
    // 当账号、角色绑定、事务提交或 policy 刷新失败时返回错误。
    //
    // `grant`：已校验的完整角色授予；`audit`：已准备的管理端资源审计。
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

    // 在同一事务中更新管理员账号并覆盖完整角色绑定。
    // 账号、角色绑定和审计更新成功时返回成功。
    //
    // 当账号、角色绑定、事务提交或 policy 刷新失败时返回错误。
    //
    // `account`：已应用领域更新的管理员实体；`grant`：已校验的替换后角色授予。
    // `audit`：已准备的管理端资源审计。
    async fn update_account_with_roles(
        &self,
        mut account: AccountCore,
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
                db.accounts().update(&mut account, executor).await?;
                rbac.assign_roles(AccountKind::Admin, &account_id, grant, executor).await?;
                Ok(())
            })
        })
        .await
    }

    // 系统引导创建根管理员时，在同一初始化事务中保存账号和内建角色绑定。
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

    // 初始化命中已有系统管理员时，按需在同一事务恢复账号并补齐 `role-root` 绑定。
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

    // 在同一事务中软删除管理员账号并清除全部角色绑定。
    // 账号和绑定均删除成功时返回 `Ok(())`。
    //
    // 当账号、角色绑定、事务提交或 policy 刷新失败时返回错误。
    //
    // `audit`：已验证的管理操作审计日志；`policy_revision`：授权快照中的 policy 版本。
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

    // 管理端写账号时按授权策略的期望版本开启事务，重验入口，并原子保存业务修改和审计。
    // 系统初始化不走此入口，须使用 `run_system_policy_transaction`。
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

    // 系统初始化按当前账号形状决定新建还是修复，目标是合法且启用的系统管理员及其 `role-root` 绑定。
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

    // 校验并原子修复已存在的超级管理员账号。
    // 返回修复后的账号与实际写入状态。
    //
    // 账号不是系统管理员时返回业务错误；角色读取、密码处理或修复写入失败时返回对应错误。
    //
    // `current`：已存在账号。
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

    // 创建系统超级管理员账号。
    // 返回新建的管理员账号。
    //
    // 当参数校验或持久化失败时返回错误。
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

    #[test]
    fn existing_super_admin_state_covers_active_inactive_and_deleted_accounts() {
        assert_eq!(ExistingSuperAdminState::from_state(false, true), ExistingSuperAdminState::Active);
        assert_eq!(ExistingSuperAdminState::from_state(false, false), ExistingSuperAdminState::Inactive);
        assert_eq!(ExistingSuperAdminState::from_state(true, true), ExistingSuperAdminState::Deleted);
        assert_eq!(ExistingSuperAdminState::from_state(true, false), ExistingSuperAdminState::Deleted);
    }

    #[test]
    fn inactive_and_deleted_super_admins_are_reported_as_reactivated() {
        assert!(!ExistingSuperAdminState::Active.reactivates());
        assert!(ExistingSuperAdminState::Inactive.reactivates());
        assert!(ExistingSuperAdminState::Deleted.reactivates());
    }
}
