//! 供应商账号开通及内部管理的调用方事务步骤。

use std::collections::HashMap;

use application_core::AuditActor;
use erp_core::AccountKind;
use erp_core::validation::normalize_required_text;
use persistence_core::Executor;
use validator::Validate;

use super::{PortalIdentityService, PreparedPortalAccount, require_internal_actor};
use crate::dto::{PortalAccountCreate, PortalAccountUpdate, PortalAccountView};
use crate::entity::portal::PortalBinding;
use crate::repository::portal::{PortalBindingRepositoryExt, PortalIdentityExt};
use crate::repository::prelude::*;
use crate::service::account_support::ensure_account_available;
use crate::service::auth::password;
use crate::{AccessControlExt, AccountCore, AccountCoreData, AccountStatus, Error, LoginAccount, Result};

impl PortalIdentityService {
    /// 在事务外规范化开通资料并计算 Argon2 密码哈希。
    ///
    /// # 参数
    /// `supplier_id` 为内部已选目标；`params` 为固定允许字段。
    /// # 返回
    /// 可进入调用方事务的账号资料，不创建角色和组织。
    /// # 错误
    /// 输入非法或密码哈希失败时拒绝。
    pub async fn prepare_account(
        &self,
        supplier_id: &str,
        params: PortalAccountCreate,
    ) -> Result<PreparedPortalAccount> {
        params.validate()?;
        let supplier_id =
            normalize_required_text(supplier_id.into(), "供应商标识不能为空", 128, "供应商标识过长")?;
        let secret = password::hash_secret(LoginAccount::new(params.account)?, params.password).await?;
        let account = AccountCore::new(
            id_generator::next_id(),
            AccountCoreData {
                secret,
                name: params.name,
                kind: AccountKind::Supplier,
                status: AccountStatus::Active,
                email: None,
                phone: None,
                avatar: None,
            },
        )?;
        Ok(PreparedPortalAccount { account, supplier_id, role: params.role })
    }

    /// 在调用方已重验供应商及管理资格的事务中开通账号和唯一绑定。
    ///
    /// # 参数
    /// `prepared` 为已哈希资料；`actor` 为内部开通人；`executor` 为调用方事务。
    /// # 返回
    /// 新账号安全视图。
    /// # 错误
    /// 内外身份不符、账号冲突或任一持久化步骤失败时拒绝。
    pub async fn account_create(
        &self,
        prepared: PreparedPortalAccount,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<PortalAccountView> {
        require_internal_actor(actor)?;
        let PreparedPortalAccount { account, supplier_id, role } = prepared;
        ensure_account_available(&self.db, &LoginAccount::new(account.secret.account())?, None, executor)
            .await?;
        let binding = PortalBinding::new(
            id_generator::next_id(),
            account.base.id.clone(),
            supplier_id,
            role,
            actor.id().into(),
        )?;
        self.db.accounts().create(&account, executor).await?;
        self.db.portal_bindings().create(&binding, executor).await?;
        Ok(PortalAccountView::from_account(&account, &binding))
    }

    /// 读取内部授权供应商的全部账号，不泄露密码哈希。
    ///
    /// # 参数
    /// `supplier_id` 为已授权供应商；`executor` 为调用方执行器。
    /// # 返回
    /// 含启停及版本的稳定账号列表。
    /// # 错误
    /// 绑定损坏或持久化访问失败时拒绝。
    pub async fn account_list(
        &self,
        supplier_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Vec<PortalAccountView>> {
        let bindings = self.db.portal_bindings().supplier_bindings(supplier_id, executor).await?;
        let ids = bindings.iter().map(|binding| binding.account_id.clone()).collect::<Vec<_>>();
        let accounts = self.db.accounts().list_by_ids(&ids, executor).await?;
        let accounts: HashMap<_, _> = accounts
            .into_iter()
            .filter(|account| account.kind == AccountKind::Supplier)
            .map(|account| (account.base.id.clone(), account))
            .collect();
        let mut result = bindings
            .iter()
            .filter_map(|binding| {
                accounts
                    .get(&binding.account_id)
                    .map(|account| PortalAccountView::from_account(account, binding))
            })
            .collect::<Vec<_>>();
        result.sort_by(|left, right| left.account_id.cmp(&right.account_id));
        Ok(result)
    }

    /// 读取账号归属以便组合层执行供应商对象授权。
    ///
    /// # 参数
    /// `account_id` 为外部账号；`executor` 为调用方事务。
    /// # 返回
    /// 不携带内部事实的账号视图。
    /// # 错误
    /// 非门户账号、绑定不存在或访问失败时拒绝。
    pub async fn account_detail(
        &self,
        account_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<PortalAccountView> {
        let (account, binding) = self.management_facts(account_id, executor).await?;
        Ok(PortalAccountView::from_account(&account, &binding))
    }

    /// 在同一调用方事务中调整岗位及启停，旧会话立即失效。
    ///
    /// # 参数
    /// `account_id` 是目标；`update` 是核对版本及状态；`actor` 是内部人；`executor` 是调用方事务。
    /// # 返回
    /// 新版本账号视图。
    /// # 错误
    /// 身份、任一版本或持久化失败时拒绝，不允许跨供应商改绑。
    pub async fn account_update(
        &self,
        account_id: &str,
        update: PortalAccountUpdate,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<PortalAccountView> {
        require_internal_actor(actor)?;
        let (mut account, mut binding) = self.management_facts(account_id, executor).await?;
        if account.base.version != update.expected_account_version {
            return Err(Error::ConflictError("供应商账号已变化，请刷新后重试".into()));
        }
        binding.update(update.expected_binding_version, update.role, update.active, actor.id().into())?;
        account.status = if update.active { AccountStatus::Active } else { AccountStatus::Suspended };
        self.db.accounts().update(&mut account, executor).await?;
        self.db.portal_bindings().update(&mut binding, executor).await?;
        Ok(PortalAccountView::from_account(&account, &binding))
    }

    /// 读取账号和绑定正式事实；只有外部账号允许进入管理步骤。
    async fn management_facts(
        &self,
        account_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<(AccountCore, PortalBinding)> {
        let missing = || Error::NotFound("供应商账号不存在".into());
        let account = self
            .db
            .accounts()
            .find_by_id(account_id, executor)
            .await?
            .filter(|account| account.kind == AccountKind::Supplier)
            .ok_or_else(missing)?;
        let binding =
            self.db.portal_bindings().binding_for_account(account_id, executor).await?.ok_or_else(missing)?;
        Ok((account, binding))
    }
}
