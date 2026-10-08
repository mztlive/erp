//! 决定写入的身份、供应商启用与内部对象范围重验。

use application_core::AuditActor;
use erp_core::AccountKind;
use erp_identity::repository::prelude::*;
use erp_identity::{AccessControlExt, Permission, PortalActor, PortalIdentityService, subject};
use erp_supplier::{SupplierAccount, SupplierExt};
use persistence_core::Executor;

use super::SupplierPortalProcess;
use crate::adapters::supplier_access;
use crate::{Error, Result};

impl SupplierPortalProcess {
    /// 在调用方执行器中重验旧会话及供应商状态。
    /// # 参数
    /// * `actor` - JWT中的原账号和绑定版本。
    /// * `executor` - 当前请求或事务执行器。
    /// # 返回
    /// 返回当前真实门户身份。
    /// # 错误
    /// 停用、解绑、版本失效或供应商停用时拒绝。
    pub async fn session_validate(
        &self,
        actor: &PortalActor,
        executor: &mut dyn Executor,
    ) -> Result<PortalActor> {
        let current = PortalIdentityService::new(self.db.clone())
            .validate_session(
                &actor.account_id,
                &actor.account,
                actor.account_version,
                actor.binding_version,
                executor,
            )
            .await?;
        self.active_supplier(&current.supplier_id, executor).await?;
        Ok(current)
    }

    /// 读取仍启用的供应商角色。
    ///
    /// # 参数
    /// * `supplier_id` - 供应商角色 ID。
    /// * `executor` - 调用方执行器。
    ///
    /// # 返回
    /// 返回仍启用的供应商角色。
    ///
    /// # 错误
    /// 供应商不存在时返回 `NotFound`；已停用时返回 `Forbidden`。读取失败时返回对应错误。
    pub(super) async fn active_supplier(
        &self,
        supplier_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<SupplierAccount> {
        let supplier = self
            .db
            .supplier_accounts()
            .find_by_id(supplier_id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("供应商不存在".into()))?;
        if !supplier.is_active() {
            return Err(Error::Forbidden("供应商已停用".into()));
        }
        Ok(supplier)
    }

    /// 重验内部账号对指定动作和供应商的对象范围，并要求供应商仍启用。
    ///
    /// # 参数
    /// * `actor` - 内部操作人。
    /// * `action` - 供应商对象动作。
    /// * `supplier_id` - 供应商角色 ID。
    /// * `executor` - 调用方执行器。
    ///
    /// # 返回
    /// 返回仍启用且在范围内的供应商角色。
    ///
    /// # 错误
    /// 不是可登录的内部账号、对象范围不足、供应商不存在或已停用时返回对应错误。
    pub(super) async fn internal_supplier(
        &self,
        actor: &AuditActor,
        action: &str,
        supplier_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<SupplierAccount> {
        self.internal_actor(actor, executor).await?;
        supplier_access(self.db.clone(), self.rbac.clone())
            .require_with(actor, action, supplier_id, executor)
            .await?;
        self.active_supplier(supplier_id, executor).await
    }

    /// 证明操作人是仍可登录的内部管理员账号。
    ///
    /// # 参数
    /// * `actor` - 声称的内部操作人。
    /// * `executor` - 调用方执行器。
    ///
    /// # 返回
    /// 账号种类与登录状态仍匹配时无返回值。
    ///
    /// # 错误
    /// 不是内部账号时返回 `Forbidden`；账号不存在或已失效时返回 `Unauthenticated`。读取失败时返回对应错误。
    pub(super) async fn internal_actor(&self, actor: &AuditActor, executor: &mut dyn Executor) -> Result<()> {
        if actor.kind() != AccountKind::Admin {
            return Err(Error::Forbidden("供应商身份不能操作内部审核".into()));
        }
        let current = self
            .db
            .accounts()
            .find_account(actor.id(), executor)
            .await?
            .ok_or_else(|| Error::Unauthenticated("内部账号不存在".into()))?;
        if current.kind != actor.kind() || !current.can_login() {
            return Err(Error::Unauthenticated("内部账号已失效".into()));
        }
        Ok(())
    }

    /// 重验原提交人仍绑定同一供应商且具备门户写权限。
    ///
    /// # 参数
    /// * `supplier_id` - 申请所属供应商。
    /// * `actor_id` - 原提交人账号 ID。
    /// * `executor` - 调用方执行器。
    ///
    /// # 返回
    /// 会话仍绑定该供应商且允许写入时无返回值。
    ///
    /// # 错误
    /// 会话失效、供应商绑定已变，或当前身份不可写时返回对应错误。
    pub(super) async fn submission_actor(
        &self,
        supplier_id: &str,
        actor_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let identity = PortalIdentityService::new(self.db.clone());
        let account = identity.account_detail(actor_id, executor).await?;
        let current = identity
            .validate_session(
                &account.account_id,
                &account.account,
                account.account_version,
                account.binding_version,
                executor,
            )
            .await?;
        if current.supplier_id != supplier_id {
            return Err(Error::Forbidden("原始提交人供应商绑定已失效".into()));
        }
        current.require_write()?;
        Ok(())
    }

    /// 按门户动作重验当前策略下的内部权限，并钉住事务内策略版本。
    ///
    /// # 参数
    /// * `actor` - 内部操作人。
    /// * `action` - 门户命令动作；未知动作按申请审核权限检查。
    /// * `executor` - 调用方执行器，用于核对策略快照。
    ///
    /// # 返回
    /// 权限存在且策略快照未漂移时无返回值。
    ///
    /// # 错误
    /// 权限码无法解析时返回 `ValidationError`；没有权限时返回 `Forbidden`。策略读取或快照核对失败时返回对应错误。
    pub(super) async fn internal_permission(
        &self,
        actor: &AuditActor,
        action: &str,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let code = match action {
            "supplier_portal.account_create" => "supplier_portal_account:create",
            "supplier_portal.account_update" => "supplier_portal_account:update",
            "supplier_portal.quote_access_update" => "supplier_portal_catalog:update",
            "supplier_portal.application_detail" => "supplier_portal_request:detail",
            _ => "supplier_portal_request:review",
        };
        let permission =
            Permission::parse(code).map_err(|error| Error::ValidationError(error.to_string()))?;
        let revision = self.rbac.current_policy_revision().await?;
        if !self.rbac.enforce(&subject(actor.kind(), actor.id()), &permission).await? {
            return Err(Error::Forbidden("当前账号缺少供应商门户管理或确认权限".into()));
        }
        self.rbac.ensure_policy_snapshot_with_executor(revision, executor).await?;
        Ok(())
    }

    /// 把内部处理人账号还原为仍可登录的管理员审计身份。
    ///
    /// # 参数
    /// * `owner` - 内部处理人账号 ID。
    /// * `executor` - 调用方执行器。
    ///
    /// # 返回
    /// 返回该账号的 `AuditActor`。
    ///
    /// # 错误
    /// 账号不存在、不是管理员或不能登录时返回 `Forbidden`。读取失败时返回对应错误。
    pub(super) async fn reviewer_actor(
        &self,
        owner: &str,
        executor: &mut dyn Executor,
    ) -> Result<AuditActor> {
        let account = self
            .db
            .accounts()
            .find_account(owner, executor)
            .await?
            .filter(|a| a.kind == AccountKind::Admin && a.can_login())
            .ok_or_else(|| Error::Forbidden("请先配置当前启用且具确认资格的内部处理人".into()))?;
        Ok(AuditActor::new(account.base.id, account.secret.account().to_string(), account.kind))
    }
}
