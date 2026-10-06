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
