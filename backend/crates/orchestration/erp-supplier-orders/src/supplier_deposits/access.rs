//! 财务资金的主体和内部组织必须由同一有效角色范围覆盖。
use std::sync::Arc;

use application_core::AuditActor;
use erp_audit::AuditActorLogs;
use erp_audit::execution::persist_log;
use erp_finance::entity::supplier_deposit::{DepositAccountIdentity, SupplierDepositAccount};
use erp_finance::repository::SupplierDepositExt;
use erp_identity::{AccessTarget, AuthorizationService, Permission};
use erp_permissions::supplier_deposit;
use mongodb::Database;
use persistence_core::Executor;
use sha2::{Digest, Sha256};

use super::repository::authorization::identity_facts;
use crate::{Error, Result};

pub(super) async fn account_access(
    db: &Database,
    authorization: &Arc<AuthorizationService>,
    id: &str,
    actor: &AuditActor,
    action: &str,
    executor: &mut dyn Executor,
) -> Result<SupplierDepositAccount> {
    authorization.require(actor, &deposit_permission(action)?, AccessTarget::record(id)?, executor).await?;
    let account = db
        .supplier_deposit_accounts()
        .find_by_id(id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("预存款账户不存在".into()))?;
    Ok(account)
}

/// 为原业务详情证明资金账号的主体及组织读取资格。
///
/// - `db` / `authorization`：应用数据及权限服务。
/// - `account_id` / `actor`：服务端资金账号及当前认证人员。
/// - `executor`：原读取事务执行器，不另开事务。
///
/// 返回已获同角色主体和组织范围许可的资金账号。
///
/// 越主体、组织或缺资金明细权限时拒绝且不交付金额。
pub async fn authorize_deposit_account(
    db: &Database,
    authorization: &Arc<AuthorizationService>,
    account_id: &str,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<SupplierDepositAccount> {
    account_access(db, authorization, account_id, actor, "detail", executor).await
}

pub(super) async fn identity_access(
    db: &Database,
    authorization: &Arc<AuthorizationService>,
    identity: &DepositAccountIdentity,
    actor: &AuditActor,
    action: &str,
    executor: &mut dyn Executor,
) -> Result<()> {
    let payload = serde_json::to_vec(identity).map_err(|error| Error::Internal(error.to_string()))?;
    let target = hex::encode(Sha256::digest(payload));
    let candidate = identity_facts(db, &target, identity, &target, executor).await?;
    authorization
        .require(actor, &deposit_permission(action)?, AccessTarget::Candidate(candidate), executor)
        .await?;
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

fn deposit_permission(action: &str) -> Result<Permission> {
    let permission = match action {
        "list" => supplier_deposit::LIST,
        "detail" => supplier_deposit::DETAIL,
        "create" => supplier_deposit::OPEN,
        "recharge_request" => supplier_deposit::REQUEST_RECHARGE,
        "approve_recharge" => supplier_deposit::APPROVE_RECHARGE,
        "pay_recharge" => supplier_deposit::PAY_RECHARGE,
        "receive_recharge" => supplier_deposit::RECEIVE_RECHARGE,
        "authorize" => supplier_deposit::AUTHORIZE,
        "revoke" => supplier_deposit::REVOKE,
        "reconcile" => supplier_deposit::RECONCILE,
        "resolve" => supplier_deposit::RESOLUTION_RESOLVE,
        "release" => supplier_deposit::RESOLUTION_RELEASE,
        "reserve" => supplier_deposit::RESERVE,
        "use" => supplier_deposit::SETTLE,
        "refund" => supplier_deposit::RETURN_USAGE,
        "bind" => supplier_deposit::BIND,
        _ => return Err(Error::ValidationError("未知预存款动作".into())),
    };
    Ok(permission)
}
