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
    /// 开通前在事务外校验账号开通字段，规范化登录名及内部选定的供应商 ID，并生成 Argon2 凭证。
    /// 输入或哈希失败时不生成开通资料；此步不写入账号，也不创建角色和组织。
    pub async fn prepare_account(
        &self,
        supplier_id: &str,
        params: PortalAccountCreate,
    ) -> Result<PreparedPortalAccount> {
        params.validate()?;
        let supplier_id =
            normalize_required_text(supplier_id.into(), "供应商标识不能为空", 128, "供应商标识过长")?;

        let secret = password::hash_secret(LoginAccount::new(params.account)?, params.password).await?;
        let account =
            AccountCore::new(id_generator::next_id(), AccountCoreData::supplier(secret, params.name))?;
        Ok(PreparedPortalAccount { account, supplier_id, role: params.role })
    }

    /// 调用方在事务中重验供应商及管理资格后，由内部操作人落地已哈希的账号及唯一供应商绑定。
    /// 成功后返回不含凭证的账号视图；身份不符、登录名冲突或任一写入失败时拒绝。
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

    /// 调用方取得供应商授权后，用原执行器读取门户账号，按账号 ID 排序并返回启停状态和版本。
    /// 视图不含密码哈希，只收录能关联到供应商账号的绑定；绑定解码或仓储读取失败时返回错误。
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

    /// 在调用方事务中按门户账号读取归属，供组合层执行供应商对象授权。
    /// 返回不含内部事实的账号视图；非门户账号、缺少绑定或读取失败时拒绝。
    pub async fn account_detail(
        &self,
        account_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<PortalAccountView> {
        let (account, binding) = self.management_facts(account_id, executor).await?;
        Ok(PortalAccountView::from_account(&account, &binding))
    }

    /// 内部操作人调整岗位或启停时，在调用方事务中核对账号与绑定版本，不允许跨供应商改绑。
    /// 成功后返回新版本视图，旧会话失效；身份不符、版本冲突或写入失败时拒绝。
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

    // 查看或调整门户账号时读取真实账号与绑定，非供应商账号或缺少绑定均视为不存在。
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
