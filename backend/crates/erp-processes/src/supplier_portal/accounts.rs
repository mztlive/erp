//! 内部开通和停用：账号与供应商对象授权分别重验。

use application_core::AuditActor;
use erp_audit::AuditActorLogs;
use erp_identity::{
    PortalAccountCreate, PortalAccountUpdate, PortalAccountView, PortalActor, PortalIdentityService,
    PortalPasswordUpdate,
};
use persistence_core::{NoTransaction, Transactional};
use serde_json::json;
use validator::Validate;

use super::SupplierPortalProcess;
use crate::Result;
use crate::audit::persist_log;

impl SupplierPortalProcess {
    /// 在内部对象范围内读取指定供应商具名账号。
    /// # 参数
    /// * `supplier_id` / `actor` - 目标供应商与内部操作人。
    /// # 返回
    /// 返回安全账号视图，不含密码或权限秘密。
    /// # 错误
    /// 无对象范围或账号状态失效时拒绝。
    pub async fn account_list(
        &self,
        supplier_id: &str,
        actor: &AuditActor,
    ) -> Result<Vec<PortalAccountView>> {
        self.internal_supplier(actor, "detail", supplier_id, &mut NoTransaction).await?;
        Ok(PortalIdentityService::new(self.db.clone()).account_list(supplier_id, &mut NoTransaction).await?)
    }

    /// 允许当前实名账号自助修改密码，密码计算在事务之外。
    /// # 参数
    /// * `actor` / `input` - 原有效会话、原密码及新密码。
    /// # 返回
    /// 返回更新后的会话版本，协议层重新签发会话。
    /// # 错误
    /// 原密码不符、绑定或供应商失效时拒绝。
    pub async fn password_update(
        &self,
        actor: &PortalActor,
        input: &PortalPasswordUpdate,
    ) -> Result<PortalActor> {
        input.validate()?;
        let prepared = PortalIdentityService::new(self.db.clone()).prepare_password(actor, input).await?;
        let this = self.clone();
        let actor = actor.clone();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    this.session_validate(&actor, executor).await?;
                    let current = PortalIdentityService::new(this.db.clone())
                        .password_update(prepared, executor)
                        .await?;
                    let log = current.audit_actor().resource_log(
                        "supplier_portal.password_update",
                        "supplier_portal_request",
                        current.account_id.clone(),
                    )?;
                    persist_log(&this.db, &log, executor).await?;
                    Ok(current)
                })
            })
            .await
    }

    /// 为已启用供应商开通具名门户账号。
    /// # 参数
    /// * `supplier_id` / `params` - 已有供应商和账号资料。
    /// * `idempotency_key` / `actor` - 原命令号及内部操作人。
    /// # 返回
    /// 返回原成功账号结果，密码不进入响应或审计。
    /// # 错误
    /// 无对象范围、供应商停用、账号冲突时拒绝。
    pub async fn account_create(
        &self,
        supplier_id: &str,
        params: PortalAccountCreate,
        idempotency_key: &str,
        actor: &AuditActor,
    ) -> Result<PortalAccountView> {
        let payload = json!({"supplier_id":supplier_id,"input":{"account":params.account,"name":params.name,"password":params.password,"role":params.role}});
        let prepared =
            PortalIdentityService::new(self.db.clone()).prepare_account(supplier_id, params).await?;
        self.internal_command(
            actor,
            supplier_id,
            "supplier_portal.account_create",
            idempotency_key,
            &payload,
            move |this, actor, executor| {
                Box::pin(async move {
                    Ok(PortalIdentityService::new(this.db).account_create(prepared, &actor, executor).await?)
                })
            },
        )
        .await
    }

    /// 修改门户角色或同步停用账号及绑定。
    /// # 参数
    /// * `account_id` / `params` - 目标账号及原账号绑定版本。
    /// * `idempotency_key` / `actor` - 原命令号及内部操作人。
    /// # 返回
    /// 返回已保存的账号状态。
    /// # 错误
    /// 原版本失效或无供应商范围时拒绝。
    pub async fn account_update(
        &self,
        account_id: &str,
        params: PortalAccountUpdate,
        idempotency_key: &str,
        actor: &AuditActor,
    ) -> Result<PortalAccountView> {
        let service = PortalIdentityService::new(self.db.clone());
        let current = service.account_detail(account_id, &mut NoTransaction).await?;
        let payload = json!({"account_id":account_id,"supplier_id":current.supplier_id,"input":params});
        let account_id = account_id.to_string();
        self.internal_command(
            actor,
            &current.supplier_id,
            "supplier_portal.account_update",
            idempotency_key,
            &payload,
            move |this, actor, executor| {
                Box::pin(async move {
                    Ok(PortalIdentityService::new(this.db)
                        .account_update(&account_id, params, &actor, executor)
                        .await?)
                })
            },
        )
        .await
    }
}
