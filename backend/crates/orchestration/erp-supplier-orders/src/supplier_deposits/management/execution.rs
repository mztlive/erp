//! 财务显式占用、实际扣款和核销分步保留，失败不得丢弃已发生资金。

use application_core::AuditActor;
use erp_core::common::time::Instant;
use erp_core::money::Amount;
use erp_finance::entity::supplier_deposit::{
    DepositReservationBasis, SupplierDepositReservation, SupplierDepositUsage,
};
use erp_finance::repository::{PurchasePaymentRequestExt, SupplierDepositExt};
use erp_finance::service::supplier_deposit::SupplierDepositService;
use erp_party::SensitiveDataCodec;
use erp_procurement::repository::PurchaseOrderExt;
use erp_supply::api_order::{ApiOrderExecution, ApiOrderExt};
use erp_supply::repository::{SupplierApiExt, SupplierFulfillmentExt};
use mongodb::Database;
use persistence_core::{Executor, Transactional};

use super::super::OrdersPaymentTasks;
use super::super::access::{account_access, audit};
use super::{
    BindDepositConnection, DepositManagementContext, RecordDepositDebit, ReserveDepositExecution,
    ReturnDepositUsage, SettleDepositUsage, SupplierDepositManagementProcess,
};
use crate::supplier_api_orders::execution_budget;
use crate::{Error, Result};

impl SupplierDepositManagementProcess {
    /// 按真实冻结执行组预算占用，不接受客户端指定金额或资金归属。
    /// # 参数
    /// * `account` / `command` / `actor` - 原账户、冻结来源和财务操作人。
    /// # 返回
    /// 返回带原批次和审批依据的专属资金占用。
    /// # 错误
    /// 采购、连接、账号、预算、审批或授权不符时拒绝。
    pub async fn reserve(
        &self,
        account: &str,
        command: ReserveDepositExecution,
        actor: &AuditActor,
    ) -> Result<SupplierDepositReservation> {
        let context = self.command_context(account, actor);
        context
            .db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move { apply_reserve(context, command, executor).await })
            })
            .await
    }

    /// 单独保存已核实实际扣款，正式核销错误不能回滚本资金证据。
    /// # 参数
    /// * `account` / `command` / `actor` - 原账户、稳定扣款证据和核验人。
    /// # 返回
    /// 返回扣款已登记但尚未核销的真实使用。
    /// # 错误
    /// 越范围、跨账户、原交易无稳定身份或写入失败时拒绝。
    pub async fn record_debit(
        &self,
        account: &str,
        command: RecordDepositDebit,
        actor: &AuditActor,
    ) -> Result<SupplierDepositUsage> {
        let context = self.command_context(account, actor);
        context
            .db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move { apply_record_debit(context, command, executor).await })
            })
            .await
    }

    /// 将已保存扣款核销到原批准采购，错误保留原资金待处理。
    /// # 参数
    /// * `account` / `usage` / `command` / `actor` - 原账户、使用和本次核销分配。
    /// # 返回
    /// 返回正式核销后的使用记录。
    /// # 错误
    /// 超批准、超实际使用、越范围或应付变化时拒绝且保留原扣款。
    pub async fn settle_usage(
        &self,
        account: &str,
        usage: &str,
        command: SettleDepositUsage,
        actor: &AuditActor,
    ) -> Result<SupplierDepositUsage> {
        let context = self.command_context(account, actor);
        let usage = usage.to_string();
        context
            .db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move { apply_settle_usage(context, command, usage, executor).await })
            })
            .await
    }

    /// 按稳定交易核实实际账户返款，并冲回原核销。
    /// # 参数
    /// * `account` / `command` / `actor` - 原账户、原使用和到账证据。
    /// # 返回
    /// 返回保留原来源的返款及冲回结果。
    /// # 错误
    /// 越范围、重复超返或证据不完整时拒绝；不依据取消消息生成到账。
    pub async fn return_usage(
        &self,
        account: &str,
        command: ReturnDepositUsage,
        actor: &AuditActor,
    ) -> Result<SupplierDepositUsage> {
        let context = self.command_context(account, actor);
        context
            .db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move { apply_return_usage(context, command, executor).await })
            })
            .await
    }

    /// 绑定同供应商资金账号，部署运行时仍须核验G0资金账号证据。
    /// # 参数
    /// * `account` / `command` / `actor` - 已核验账户及服务器连接身份。
    /// # 返回
    /// 绑定成功，不启用新增下单或自动授权。
    /// # 错误
    /// 跨供应商、已有其他账号或越范围时拒绝。
    pub async fn bind_connection(
        &self,
        account: &str,
        command: BindDepositConnection,
        actor: &AuditActor,
    ) -> Result<()> {
        let context = self.command_context(account, actor);
        context
            .db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move { apply_bind_connection(context, command, executor).await })
            })
            .await
    }
}

fn known_external_order(execution: &ApiOrderExecution, original_root: Option<&str>, order_id: &str) -> bool {
    original_root == Some(order_id)
        || execution
            .order_tree
            .as_ref()
            .is_some_and(|tree| tree.nodes.iter().any(|node| node.external_order_no == order_id))
        || execution.pending_order_tree.as_ref().is_some_and(|tree| {
            original_root == Some(tree.root_order_no.as_str())
                && tree.nodes.iter().any(|node| node.external_order_no == order_id)
        })
}

async fn sync_usage_payment_task(
    db: &Database,
    usage: &SupplierDepositUsage,
    tasks: &dyn OrdersPaymentTasks,
    executor: &mut dyn Executor,
) -> Result<()> {
    let reservation = db
        .supplier_deposit_reservations()
        .find_by_id(&usage.reservation_id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("原资金占用不存在".into()))?;
    match tasks.sync(&reservation.basis.payment_request_id, executor).await {
        Ok(()) => Ok(()),
        Err(Error::BusinessLogicError(_) | Error::ConflictError(_) | Error::NotFound(_)) => {
            let mut account = db
                .supplier_deposit_accounts()
                .find_by_id(&usage.account_id, executor)
                .await?
                .ok_or_else(|| Error::NotFound("原预存款账户不存在".into()))?;
            account.flag_reconciliation_risk(Instant::now())?;
            db.supplier_deposit_accounts().update(&mut account, executor).await?;
            Ok(())
        },
        Err(error) => Err(error),
    }
}

async fn apply_reserve(
    context: DepositManagementContext,
    command: ReserveDepositExecution,
    executor: &mut dyn Executor,
) -> Result<SupplierDepositReservation> {
    let DepositManagementContext { db, authorization, account, actor, codec, .. } = context;

    let funds_account = account_access(&db, &authorization, &account, &actor, "reserve", executor).await?;
    let mut execution = db
        .supplier_api_order_executions()
        .find_by_id(&command.execution_group_id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("采购接口执行组不存在".into()))?;
    let connection = db
        .supplier_api_connections()
        .find_by_id(&execution.connection_id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("接口连接不存在".into()))?;
    if connection.supplier_deposit_account_id.as_deref() != Some(&account)
        || connection.supplier_deposit_external_account_ref.as_ref()
            != Some(&funds_account.identity.external_account_ref)
        || execution.supplier_id != funds_account.identity.supplier_id
    {
        return Err(Error::BusinessLogicError("执行组连接与当前资金账户归属不一致".into()));
    }

    let purchase = db
        .purchase_orders()
        .find_by_id(&execution.purchase_order_id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("采购单不存在".into()))?;
    if purchase.stable.current_revision_id.as_deref() != Some(&execution.purchase_order_revision_id)
        || execution.superseded_by_purchase_revision_id.is_some()
    {
        return Err(Error::ConflictError("采购生效版本或执行组已变化".into()));
    }

    let basis = reservation_basis(&db, &account, &command, &execution, &codec, executor).await?;
    let result = SupplierDepositService::new(db.clone())
        .reserve(&account, &command.operation_key, basis, actor.id(), Instant::now(), executor)
        .await?;

    execution.next_run_at = Instant::now();
    execution.blocker = None;
    db.supplier_api_order_executions().update(&mut execution, executor).await?;
    audit(&db, &actor, "reserve", &account, executor).await?;
    Ok(result)
}

async fn apply_record_debit(
    context: DepositManagementContext,
    command: RecordDepositDebit,
    executor: &mut dyn Executor,
) -> Result<SupplierDepositUsage> {
    let DepositManagementContext { db, authorization, account, actor, .. } = context;

    account_access(&db, &authorization, &account, &actor, "use", executor).await?;
    let reservation = db
        .supplier_deposit_reservations()
        .find_by_id(&command.reservation_id, executor)
        .await?
        .filter(|record| record.account_id == account)
        .ok_or_else(|| Error::NotFound("原资金占用不存在".into()))?;
    let execution = db
        .supplier_api_order_executions()
        .find_by_id(&reservation.basis.execution_group_id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("原采购执行记录不存在".into()))?;
    let original = db
        .supplier_fulfillment_orders()
        .find_by_id(&execution.base.id, executor)
        .await?
        .filter(|order| order.connection_id.as_ref() == execution.connection_id);
    if !known_external_order(
        &execution,
        original.as_ref().and_then(|order| order.external_order_no.as_deref()),
        &command.evidence.external_order_id,
    ) {
        return Err(Error::BusinessLogicError(
            "扣款订单尚未与原采购执行组核实关联，请按账户未决资金证据登记".into(),
        ));
    }

    let result = SupplierDepositService::new(db.clone())
        .record_debit(
            &command.reservation_id,
            &command.operation_key,
            command.amount,
            command.evidence,
            actor.id(),
            executor,
        )
        .await?;
    audit(&db, &actor, "use", &account, executor).await?;
    Ok(result)
}

async fn apply_settle_usage(
    context: DepositManagementContext,
    command: SettleDepositUsage,
    usage: String,
    executor: &mut dyn Executor,
) -> Result<SupplierDepositUsage> {
    let DepositManagementContext { db, authorization, account, actor, payment_tasks, .. } = context;

    account_access(&db, &authorization, &account, &actor, "use", executor).await?;
    db.supplier_deposit_usages()
        .find_by_id(&usage, executor)
        .await?
        .filter(|source| source.account_id == account)
        .ok_or_else(|| Error::NotFound("原预存款使用不存在".into()))?;

    let result = SupplierDepositService::new(db.clone())
        .settle_usage(&usage, &command.operation_key, &command.allocations, actor.id(), executor)
        .await?;
    sync_usage_payment_task(&db, &result, payment_tasks.as_ref(), executor).await?;
    audit(&db, &actor, "settle", &account, executor).await?;
    Ok(result)
}

async fn apply_return_usage(
    context: DepositManagementContext,
    command: ReturnDepositUsage,
    executor: &mut dyn Executor,
) -> Result<SupplierDepositUsage> {
    let DepositManagementContext { db, authorization, account, actor, payment_tasks, .. } = context;

    account_access(&db, &authorization, &account, &actor, "refund", executor).await?;
    db.supplier_deposit_usages()
        .find_by_id(&command.usage_id, executor)
        .await?
        .filter(|usage| usage.account_id == account)
        .ok_or_else(|| Error::NotFound("原资金使用不存在".into()))?;

    let result = SupplierDepositService::new(db.clone())
        .return_usage(
            &command.usage_id,
            &command.operation_key,
            command.amount,
            command.evidence,
            actor.id(),
            executor,
        )
        .await?;
    sync_usage_payment_task(&db, &result, payment_tasks.as_ref(), executor).await?;
    audit(&db, &actor, "refund", &account, executor).await?;
    Ok(result)
}

async fn apply_bind_connection(
    context: DepositManagementContext,
    command: BindDepositConnection,
    executor: &mut dyn Executor,
) -> Result<()> {
    let DepositManagementContext { db, authorization, account, actor, .. } = context;

    let source = account_access(&db, &authorization, &account, &actor, "bind", executor).await?;
    let mut connection = db
        .supplier_api_connections()
        .find_by_id(&command.connection_id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("供应商连接不存在".into()))?;
    if connection.supplier_id.as_ref() != source.identity.supplier_id {
        return Err(Error::Forbidden("资金账户与接口连接不属于同一供应商".into()));
    }

    connection.bind_supplier_deposit(account.clone(), source.identity.external_account_ref, actor.id())?;
    db.supplier_api_connections().update(&mut connection, executor).await?;
    audit(&db, &actor, "bind", &account, executor).await
}

async fn reservation_basis(
    db: &Database,
    account: &str,
    command: &ReserveDepositExecution,
    execution: &ApiOrderExecution,
    codec: &SensitiveDataCodec,
    executor: &mut dyn Executor,
) -> Result<DepositReservationBasis> {
    let request = db
        .purchase_payment_requests()
        .find_by_id(&command.payment_request_id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("付款申请不存在".into()))?;
    request.require_deposit_account(account)?;
    let line = request
        .lines
        .iter()
        .find(|line| {
            line.purchase_order_id == execution.purchase_order_id
                && line.purchase_revision_id == execution.purchase_order_revision_id
        })
        .ok_or_else(|| Error::BusinessLogicError("执行采购不在原批准付款申请范围内".into()))?;

    let budget = execution_budget(execution, codec)?;
    if budget.approved_other != Amount::zero() {
        return Err(Error::BusinessLogicError("当前余额执行尚不支持其他收费项目".into()));
    }

    let authorization = db
        .supplier_deposit_authorizations()
        .find_by_id(&command.authorization_id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("自动余额执行授权不存在".into()))?;
    Ok(DepositReservationBasis {
        payment_request_id: request.base.id.clone(),
        payment_request_version: request.submission_version,
        payable_account_id: line.payable_account_id.clone(),
        purchase_order_id: execution.purchase_order_id.clone(),
        purchase_revision_id: execution.purchase_order_revision_id.clone(),
        execution_group_id: execution.base.id.clone(),
        approved_goods_amount: budget.approved_goods,
        approved_freight: budget.approved_freight,
        authorization_id: authorization.base.id,
        authorization_version: authorization.grant_version,
    })
}
