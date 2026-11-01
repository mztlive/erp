//! 财务资金的主体和内部组织必须由同一有效角色范围覆盖。

use application_core::AuditActor;
use erp_audit::AuditActorLogs;
use erp_audit::execution::persist_log;
use erp_finance::entity::supplier_deposit::{DepositAccountIdentity, SupplierDepositAccount};
use erp_finance::repository::SupplierDepositExt;
use erp_identity::SharedRbacService;
use erp_identity::access_control::ScopedObject;
use erp_identity::service::access_control::resolve::DataScopeService;
use erp_party::PartyExt;
use erp_supplier::SupplierExt;
use mongodb::Database;
use persistence_core::Executor;

use crate::{Error, Result};

pub(super) async fn account_access(
    db: &Database,
    rbac: &SharedRbacService,
    id: &str,
    actor: &AuditActor,
    action: &str,
    executor: &mut dyn Executor,
) -> Result<SupplierDepositAccount> {
    let account = db
        .supplier_deposit_accounts()
        .find_by_id(id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("预存款账户不存在".into()))?;
    identity_access(db, rbac, &account.identity, actor, action, executor).await?;
    Ok(account)
}

/// 为原业务详情证明资金账号的主体及组织读取资格。
/// # 参数
/// * `db` / `rbac` - 应用数据及权限服务。
/// * `account_id` / `actor` - 服务端资金账号及当前认证人员。
/// * `executor` - 原读取事务执行器，不另开事务。
/// # 返回
/// 返回已获同角色主体和组织范围许可的资金账号。
/// # 错误
/// 越主体、组织或缺资金明细权限时拒绝且不交付金额。
pub async fn authorize_deposit_account(
    db: &Database,
    rbac: &SharedRbacService,
    account_id: &str,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<SupplierDepositAccount> {
    account_access(db, rbac, account_id, actor, "detail", executor).await
}

pub(super) async fn identity_access(
    db: &Database,
    rbac: &SharedRbacService,
    identity: &DepositAccountIdentity,
    actor: &AuditActor,
    action: &str,
    executor: &mut dyn Executor,
) -> Result<()> {
    let payer = db
        .parties()
        .find_by_id(&identity.payer_party_id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("我方付款主体不存在".into()))?;
    if !payer.is_active() || payer.company_profile.is_none() {
        return Err(Error::BusinessLogicError("预存款付款主体必须是有效的我方公司".into()));
    }
    let supplier = db
        .supplier_accounts()
        .find_by_id(&identity.supplier_id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("供应商不存在".into()))?;
    let access = DataScopeService::new(db.clone(), rbac.clone())
        .resolve(actor, "supplier_deposit", action, executor)
        .await?;
    let object = ScopedObject {
        owned: false,
        collaborating: false,
        historical_read_participant: false,
        org_unit_id: Some(&supplier.business_org_unit_id),
        settlement_party_id: Some(&identity.payer_party_id),
        warehouse_id: None,
    };
    if !access.scope.allows(&object, false) {
        return Err(Error::Forbidden("预存款账户不在当前付款主体和供应商组织权限范围内".into()));
    }
    Ok(())
}

pub(super) async fn audit(
    db: &Database,
    actor: &AuditActor,
    action: &str,
    account: &str,
    executor: &mut dyn Executor,
) -> Result<()> {
    let log = actor.clone().resource_log(
        &format!("supplier_deposit.{action}"),
        "supplier_deposit",
        account.to_string(),
    )?;
    persist_log(db, &log, executor).await?;
    Ok(())
}
