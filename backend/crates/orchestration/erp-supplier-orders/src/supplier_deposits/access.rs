//! 财务资金的主体和内部组织必须由同一有效角色范围覆盖。

use application_core::AuditActor;
use erp_audit::AuditActorLogs;
use erp_audit::execution::persist_log;
use erp_finance::entity::supplier_deposit::{DepositAccountIdentity, SupplierDepositAccount};
use erp_finance::repository::SupplierDepositExt;
use erp_identity::{AccessSubject, AccessTarget, Permission, ResourceTargets, SharedRbacService};
use mongodb::Database;
use persistence_core::Executor;
use sha2::{Digest, Sha256};

use super::repository::authorization::authorization;
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
    let (authorization, _) = authorization(db, rbac, None);
    authorization
        .require(
            actor,
            &Permission::parse(format!("supplier_deposit:{action}"))?,
            AccessTarget::Records { subject: AccessSubject::Direct, ids: ResourceTargets::new(&[id])? },
            executor,
        )
        .await?;
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
    let payload = serde_json::to_vec(identity).map_err(|error| Error::Internal(error.to_string()))?;
    let target = hex::encode(Sha256::digest(payload));
    let (authorization, _) = authorization(db, rbac, Some((target.clone(), identity.clone())));
    authorization
        .require(
            actor,
            &Permission::parse(format!("supplier_deposit:{action}"))?,
            AccessTarget::Records { subject: AccessSubject::Direct, ids: ResourceTargets::new(&[&target])? },
            executor,
        )
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
