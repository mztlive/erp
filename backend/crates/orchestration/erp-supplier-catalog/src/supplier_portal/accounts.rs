//! 内部开通和停用：账号与供应商对象授权分别重验。

use application_core::AuditActor;
use erp_audit::AuditActorLogs;
use erp_audit::execution::persist_log;
use erp_core::ids::SupplierAccountId;
use erp_identity::{
    PortalAccountCreate, PortalAccountUpdate, PortalAccountView, PortalActor, PortalIdentityService,
    PortalPasswordUpdate,
};
use erp_permissions::{supplier, supplier_portal};
use erp_supplier::SupplierAccess;
use erp_supplier::authorization::CompiledScope;
use erp_supplier::repository::{SupplierAccountFilter, SupplierAccountRepositoryScopeExt, SupplierExt};
use mongodb::Database;
use persistence_core::{Executor, Transactional};
use serde_json::json;
use validator::Validate;

use super::SupplierPortalProcess;
use super::command::scoped_command;
use crate::{Error, Result};

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
        let process = self.clone();
        let supplier_id = supplier_id.to_owned();
        let actor = actor.clone();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let access = Self::management_supplier(
                        &process.db,
                        &process.services.suppliers,
                        &actor,
                        &supplier_id,
                        executor,
                    )
                    .await?;
                    let accounts = PortalIdentityService::new(process.db.clone())
                        .account_list(&supplier_id, executor)
                        .await?;
                    access.validate(executor).await?;
                    Ok(accounts)
                })
            })
            .await
    }

    /// 在原快照编译详情范围并确认精确供应商；凭证保留到完整列表交付。
    /// # 参数
    /// db、suppliers 为已装配的数据库及供应商授权；actor、supplier_id 为真实目标，executor 为原快照。
    /// # 返回
    /// 供应商启用且可读时返回当前查询凭证，调用方在完整读取后 validate。
    /// # 错误
    /// 无快照、无资格、范围外、供应商停用或取证失败时拒绝。
    pub(super) async fn management_supplier(
        db: &Database,
        suppliers: &SupplierAccess,
        actor: &AuditActor,
        supplier_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<CompiledScope> {
        if !executor.boundary().is_some_and(|boundary| boundary.is_snapshot()) {
            return Err(Error::ValidationError("内部门户读取必须使用快照事务".into()));
        }
        let access = suppliers.resolve(actor, &supplier::DETAIL, executor).await?;
        let filter = SupplierAccountFilter {
            keyword: None,
            party_id: None,
            party_ids: None,
            status: None,
            supplier_ids: Some(vec![SupplierAccountId::new(supplier_id)]),
            excluded_supplier_ids: None,
            authorized_scope: access.clone(),
            maintainer_user_ids: None,
            page: 1,
            page_size: 1,
            sort_by: None,
            sort_ascending: false,
        };
        if db.supplier_accounts().query_versions(&filter, executor).await?.is_empty() {
            return Err(Error::NotFound("供应商不存在或无权查看".into()));
        }
        let supplier = db
            .supplier_accounts()
            .find_by_id(supplier_id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("供应商不存在".into()))?;
        if !supplier.is_active() {
            return Err(Error::Forbidden("供应商已停用".into()));
        }
        Ok(access)
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
            ("supplier_portal.account_create", supplier::DETAIL, supplier_portal::ADMIN_CREATE_ACCOUNT),
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
        let account_id = account_id.to_string();
        let target_id = account_id.clone();
        let target_params = params.clone();
        let key = idempotency_key.to_owned();
        self.internal_target_command(
            actor,
            ("supplier_portal.account_update", supplier::DETAIL, supplier_portal::ADMIN_UPDATE_ACCOUNT),
            move |this, actor, executor| {
                Box::pin(async move {
                    let current = PortalIdentityService::new(this.db).account_detail(&target_id, executor).await?;
                    let payload = json!({"account_id":target_id,"supplier_id":current.supplier_id,"input":target_params});
                    let command = scoped_command(actor.id(), &current.supplier_id, "supplier_portal.account_update", &key, &payload)?;
                    Ok((current.supplier_id, command))
                })
            },
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

#[cfg(test)]
mod read_tests {
    use application_core::AuditActor;
    use erp_core::AccountKind;
    use erp_identity::testing::unbound_authorization;
    use erp_supplier::SupplierAccess;
    use mongodb::Client;
    use persistence_core::testing::SnapshotExecutor;
    use persistence_core::{Executor, NoTransaction, ReadExecutor};

    use super::SupplierPortalProcess;
    use crate::Error;

    /// 非快照读取在身份或供应商读取前拒绝，避免再次引入无边界授权。
    #[tokio::test]
    async fn management_lists_reject_unscoped_executors_before_reading_supplier() {
        let db = Client::with_uri_str("mongodb://127.0.0.1:1").await.unwrap().database("unit_test");
        let access = SupplierAccess::new(db.clone(), unbound_authorization());
        let actor = AuditActor::new("actor".into(), "actor".into(), AccountKind::Admin);
        for executor in [&mut NoTransaction as &mut dyn Executor, &mut ReadExecutor::default()] {
            assert!(matches!(
                SupplierPortalProcess::management_supplier(&db, &access, &actor, "supplier", executor).await,
                Err(Error::ValidationError(message)) if message == "内部门户读取必须使用快照事务"
            ));
        }
    }

    /// 同快照进入真实查询授权；缺少绑定时不得继续读取供应商或列表。
    #[tokio::test]
    async fn management_lists_stop_before_business_reads_when_query_binding_is_missing() {
        let db = Client::with_uri_str("mongodb://127.0.0.1:1").await.unwrap().database("unit_test");
        let access = SupplierAccess::new(db.clone(), unbound_authorization());
        let actor = AuditActor::new("actor".into(), "actor".into(), AccountKind::Admin);
        assert!(matches!(
            SupplierPortalProcess::management_supplier(
                &db,
                &access,
                &actor,
                "supplier",
                &mut SnapshotExecutor::default(),
            )
            .await,
            Err(Error::ValidationError(message)) if message == "缺少查询 Mapping 绑定"
        ));
    }
}
