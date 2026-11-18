//! 资金自动使用授权及对账，均不能直接改写余额。

use application_core::AuditActor;
use entity_core::BaseModel;
use erp_core::money::Amount;
use erp_finance::entity::supplier_deposit::{
    DepositAuthorizationMode, SupplierDepositAuthorization, SupplierDepositReconciliation,
};
use erp_finance::service::supplier_deposit::SupplierDepositService;
use persistence_core::{Executor, Transactional};
use sha2::{Digest, Sha256};

use super::super::access::{account_access, audit};
use super::{
    AuthorizeDepositExecution, DepositExpectedVersion, DepositManagementContext, ReconcileDepositAccount,
    SupplierDepositManagementProcess,
};
use crate::{Error, Result};

impl SupplierDepositManagementProcess {
    /// 授予指定账户有期限和累计限额的自动余额执行权。
    /// # 参数
    /// * `account` / `command` / `actor` - 资金账户、完整条款和财务授权人。
    /// # 返回
    /// 返回服务端记录授权身份的条款。
    /// # 错误
    /// 越范围、缺依据或无限额条款时拒绝。
    pub async fn authorize(
        &self,
        account: &str,
        command: AuthorizeDepositExecution,
        actor: &AuditActor,
    ) -> Result<SupplierDepositAuthorization> {
        let context = self.command_context(account, actor);
        context
            .db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move { apply_authorize(context, command, executor).await })
            })
            .await
    }

    /// 撤销新写动作授权，原占用与资金调查继续保留。
    /// # 参数
    /// * `account` / `id` / `command` / `actor` - 原归属、授权、版本和操作人。
    /// # 返回
    /// 返回已撤销授权。
    /// # 错误
    /// 越账户、越范围或版本漂移时拒绝。
    pub async fn revoke(
        &self,
        account: &str,
        id: &str,
        command: DepositExpectedVersion,
        actor: &AuditActor,
    ) -> Result<SupplierDepositAuthorization> {
        let context = self.command_context(account, actor);
        let id = id.to_string();
        context
            .db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move { apply_revoke(context, command, id, executor).await })
            })
            .await
    }

    /// 登记同一观察范围的资金对账证据；异常不自动消失。
    /// # 参数
    /// * `account` / `command` / `actor` - 归属、对账证据和核验人。
    /// # 返回
    /// 返回保留历史且不覆盖余额的对账记录。
    /// # 错误
    /// 越范围、时间不完整、重复异载荷或证据缺失时拒绝。
    pub async fn reconcile(
        &self,
        account: &str,
        command: ReconcileDepositAccount,
        actor: &AuditActor,
    ) -> Result<SupplierDepositReconciliation> {
        let context = self.command_context(account, actor);
        context
            .db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move { apply_reconcile(context, command, executor).await })
            })
            .await
    }
}

pub(super) fn operation_id(account: &str, action: &str, operation: &str) -> Result<String> {
    if operation.trim().is_empty() || operation.len() > 128 {
        return Err(Error::ValidationError("操作身份无效".into()));
    }
    let payload = serde_json::to_vec(&(account, action, operation))
        .map_err(|_| Error::Internal("操作身份编码失败".into()))?;
    Ok(hex::encode(Sha256::digest(payload)))
}

async fn apply_authorize(
    context: DepositManagementContext,
    command: AuthorizeDepositExecution,
    executor: &mut dyn Executor,
) -> Result<SupplierDepositAuthorization> {
    let DepositManagementContext { db, authorization, account, actor, .. } = context;

    account_access(&db, &authorization, &account, &actor, "authorize", executor).await?;

    let id = operation_id(&account, "authorize", &command.operation_key)?;
    let authorization = SupplierDepositAuthorization {
        base: BaseModel::new(id),
        account_id: account.clone(),
        authorized_by: actor.id().into(),
        approval_reference: command.approval_reference,
        grant_version: 1,
        valid_from: command.valid_from,
        valid_until: command.valid_until,
        per_execution_limit: command.per_execution_limit,
        cumulative_limit: command.cumulative_limit,
        committed_amount: Amount::zero(),
        purchase_order_ids: command.purchase_order_ids,
        revoked: false,
        mode: DepositAuthorizationMode::Automatic,
    };
    let result = SupplierDepositService::new(db.clone()).authorize(authorization, executor).await?;
    audit(&db, &actor, "authorize", &account, executor).await?;
    Ok(result)
}

async fn apply_revoke(
    context: DepositManagementContext,
    command: DepositExpectedVersion,
    id: String,
    executor: &mut dyn Executor,
) -> Result<SupplierDepositAuthorization> {
    let DepositManagementContext { db, authorization, account, actor, .. } = context;

    account_access(&db, &authorization, &account, &actor, "revoke", executor).await?;
    let result = SupplierDepositService::new(db.clone())
        .revoke_authorization(&account, &id, command.expected_version, executor)
        .await?;
    audit(&db, &actor, "revoke", &account, executor).await?;
    Ok(result)
}

async fn apply_reconcile(
    context: DepositManagementContext,
    command: ReconcileDepositAccount,
    executor: &mut dyn Executor,
) -> Result<SupplierDepositReconciliation> {
    let DepositManagementContext { db, authorization, account, actor, .. } = context;

    account_access(&db, &authorization, &account, &actor, "reconcile", executor).await?;

    let id = operation_id(&account, "reconcile", &command.operation_key)?;
    let record = SupplierDepositReconciliation {
        base: BaseModel::new(id),
        account_id: account.clone(),
        operation_key: command.operation_key,
        covered_from: command.covered_from,
        covered_until: command.covered_until,
        observed_at: command.observed_at,
        remote_balance: command.remote_balance,
        local_balance_at_cutoff: command.local_balance_at_cutoff,
        unresolved_transaction_ids: command.unresolved_transaction_ids,
        evidence_reference: command.evidence_reference,
        resolved_by: None,
        resolution_reference: None,
        resolution: None,
    };
    let result = SupplierDepositService::new(db.clone()).record_reconciliation(record, executor).await?;
    audit(&db, &actor, "reconcile", &account, executor).await?;
    Ok(result)
}
