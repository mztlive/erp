//! 充值申请、独立批准、真实银行支付和供应商到账分别执行。
use std::sync::Arc;

use application_core::AuditActor;
use erp_core::common::time::{BusinessDate, Instant};
use erp_core::ids::{FileAssetId, PartyBankAccountId, SupplierAccountId};
use erp_finance::entity::payable::SupplierPaymentData;
use erp_finance::entity::supplier_deposit::{
    DepositRechargeEvidence, SupplierDepositBatch, SupplierDepositRechargeRequest,
};
use erp_finance::repository::{PayableExt, SupplierDepositExt};
use erp_finance::service::supplier_deposit::{DepositRechargePaymentTarget, SupplierDepositService};
use erp_identity::AuthorizationService;
use erp_party::PartyExt;
use erp_party::repository::prelude::PartyBankAccountRepositoryExt;
use erp_read_models::finance::supplier_deposits::account_row;
use erp_supplier::SupplierExt;
use erp_support::FileAssetExt;
use erp_support::authorization::files::require_owned;
use mongodb::Database;
use persistence_core::{Executor, Transactional};

use super::super::access::{account_access, audit};
use super::{
    ApproveDepositRecharge, DepositManagementContext, PayDepositRecharge, ReceiveDepositRecharge,
    RequestDepositRecharge, SupplierDepositManagementProcess,
};
use crate::{Error, Result};

impl SupplierDepositManagementProcess {
    /// 登记本账户待批准充值申请。
    ///
    /// `account` / `command` / `actor`：当前账户、金额用途及申请人。
    ///
    /// 返回不可用的待批准申请。
    ///
    /// 越范围、金额或操作身份非法时拒绝。
    pub async fn request_recharge(
        &self,
        account: &str,
        command: RequestDepositRecharge,
        actor: &AuditActor,
    ) -> Result<SupplierDepositRechargeRequest> {
        let context = self.command_context(account, actor);
        context
            .db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move { apply_request_recharge(context, command, executor).await })
            })
            .await
    }

    /// 充值申请进入审批后，由另一位有资格的财务人员批准原申请；批准不是到账证据，不增加可用余额。
    ///
    /// `account` / `request` / `command` / `actor`：归属、原申请、版本及批准人。
    ///
    /// 返回已批准的原申请。
    ///
    /// 越主体、自己批准或原状态变化时拒绝。
    pub async fn approve_recharge(
        &self,
        account: &str,
        request: &str,
        command: ApproveDepositRecharge,
        actor: &AuditActor,
    ) -> Result<SupplierDepositRechargeRequest> {
        let context = self.command_context(account, actor);
        let request = request.to_string();
        context
            .db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move { apply_approve_recharge(context, command, request, executor).await })
            })
            .await
    }

    /// 充值获批并发生银行付款后，登记实际付款证据；尚未核对供应商到账，不能把银行已付视为资金可用。
    ///
    /// `account` / `request` / `command` / `actor`：原归属、申请及实际付款证据。
    ///
    /// 返回银行已付、供应商未到账的申请。
    ///
    /// 未批准、银行账户不属于供应商、回单越权或版本变化时拒绝。
    pub async fn pay_recharge(
        &self,
        account: &str,
        request: &str,
        command: PayDepositRecharge,
        actor: &AuditActor,
    ) -> Result<SupplierDepositRechargeRequest> {
        let context = self.command_context(account, actor);
        let request = request.to_string();
        context
            .db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move { apply_pay_recharge(context, command, request, executor).await })
            })
            .await
    }

    /// 取得供应商实际到账证据后，与原批准和银行付款核对；通过后才创建可用到账批次。
    ///
    /// `account` / `command` / `actor`：原账户、稳定外部到账与核验人。
    ///
    /// 返回首次可用的到账批次；重放恢复原批次。
    ///
    /// 充值未付款、账户归属、重复交易或金额证据不符时拒绝。
    pub async fn receive_recharge(
        &self,
        account: &str,
        command: ReceiveDepositRecharge,
        actor: &AuditActor,
    ) -> Result<SupplierDepositBatch> {
        let context = self.command_context(account, actor);
        context
            .db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move { apply_receive_recharge(context, command, executor).await })
            })
            .await
    }
}

async fn validate_bank_evidence(
    db: &Database,
    authorization: &Arc<AuthorizationService>,
    supplier: &str,
    command: &PayDepositRecharge,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<()> {
    let supplier = db
        .supplier_accounts()
        .find_by_id(supplier, executor)
        .await?
        .ok_or_else(|| Error::NotFound("供应商不存在".into()))?;
    let accounts =
        db.party_bank_accounts().list_active_on(&supplier.party_id, BusinessDate::today(), executor).await?;
    let account = accounts
        .into_iter()
        .find(|account| account.base.id == command.payee_bank_account_id)
        .ok_or_else(|| Error::BusinessLogicError("充值收款账户不属于供应商或已失效".into()))?;
    if !db
        .party_bank_accounts()
        .lock_for_payment_request(&account.base.id, account.base.version, executor)
        .await?
    {
        return Err(Error::ConflictError("充值收款账户已变化".into()));
    }

    let asset = db
        .file_assets()
        .find_by_id(&command.bank_receipt_asset_id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("银行回单不存在".into()))?;
    if !asset.is_usable_at(Instant::now()) {
        return Err(Error::Forbidden("银行回单已不可用".into()));
    }
    require_owned(authorization, actor, &[&command.bank_receipt_asset_id], executor).await?;
    Ok(())
}

async fn apply_request_recharge(
    context: DepositManagementContext,
    command: RequestDepositRecharge,
    executor: &mut dyn Executor,
) -> Result<SupplierDepositRechargeRequest> {
    let DepositManagementContext { db, authorization, account, actor, .. } = context;

    account_access(&db, &authorization, &account, &actor, "recharge_request", executor).await?;
    let result = SupplierDepositService::new(db.clone())
        .request_recharge(
            &account,
            &command.operation_key,
            command.amount,
            command.reason,
            actor.id(),
            executor,
        )
        .await?;
    audit(&db, &actor, "recharge_request", &account, executor).await?;
    Ok(result)
}

async fn apply_approve_recharge(
    context: DepositManagementContext,
    command: ApproveDepositRecharge,
    request: String,
    executor: &mut dyn Executor,
) -> Result<SupplierDepositRechargeRequest> {
    let DepositManagementContext { db, authorization, account, actor, .. } = context;

    account_access(&db, &authorization, &account, &actor, "approve_recharge", executor).await?;
    let result = SupplierDepositService::new(db.clone())
        .approve_recharge(
            &account,
            &request,
            command.expected_version,
            command.approval_reference,
            actor.id(),
            executor,
        )
        .await?;
    audit(&db, &actor, "approve_recharge", &account, executor).await?;
    Ok(result)
}

async fn apply_pay_recharge(
    context: DepositManagementContext,
    command: PayDepositRecharge,
    request: String,
    executor: &mut dyn Executor,
) -> Result<SupplierDepositRechargeRequest> {
    let DepositManagementContext { db, authorization, account, actor, .. } = context;

    let source = account_access(&db, &authorization, &account, &actor, "pay_recharge", executor).await?;
    let row = account_row(&db, source.clone(), executor).await?;
    let recharge = db
        .supplier_deposit_recharge_requests()
        .find_by_id(&request, executor)
        .await?
        .filter(|request| request.account_id == account)
        .ok_or_else(|| Error::NotFound("充值申请不存在".into()))?;
    validate_bank_evidence(&db, &authorization, &source.identity.supplier_id, &command, &actor, executor)
        .await?;

    let data = SupplierPaymentData {
        payment_no: format!("CZFK-{}", recharge.request_no),
        supplier_id: SupplierAccountId::new(source.identity.supplier_id),
        payee_bank_account_id: PartyBankAccountId::new(command.payee_bank_account_id),
        paid_at: command.paid_at,
        amount: recharge.amount,
        bank_reference: Some(command.bank_reference),
        bank_receipt_asset_id: FileAssetId::new(command.bank_receipt_asset_id),
    };
    let result = SupplierDepositService::new(db.clone())
        .register_recharge_payment(
            DepositRechargePaymentTarget {
                account_id: &account,
                request_id: &request,
                expected_version: command.expected_version,
            },
            data,
            &row.payer_party_name,
            actor.id(),
            executor,
        )
        .await?;
    audit(&db, &actor, "pay_recharge", &account, executor).await?;
    Ok(result)
}

async fn apply_receive_recharge(
    context: DepositManagementContext,
    command: ReceiveDepositRecharge,
    executor: &mut dyn Executor,
) -> Result<SupplierDepositBatch> {
    let DepositManagementContext { db, authorization, account, actor, .. } = context;

    account_access(&db, &authorization, &account, &actor, "receive_recharge", executor).await?;
    let request = db
        .supplier_deposit_recharge_requests()
        .find_by_id(&command.recharge_request_id, executor)
        .await?
        .filter(|request| request.account_id == account)
        .ok_or_else(|| Error::NotFound("充值申请不存在".into()))?;
    let payment_id = request
        .supplier_payment_id
        .ok_or_else(|| Error::BusinessLogicError("请先登记该充值申请的实际银行付款".into()))?;
    let payment = db
        .supplier_payments()
        .find_by_id(&payment_id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("实际充值付款不存在".into()))?;

    let evidence = DepositRechargeEvidence {
        approved_request_id: request.base.id,
        approval_reference: request
            .approval_reference
            .ok_or_else(|| Error::BusinessLogicError("充值缺少批准依据".into()))?,
        supplier_payment_id: payment_id,
        bank_reference: payment
            .bank_reference
            .ok_or_else(|| Error::BusinessLogicError("充值缺少银行流水".into()))?,
        bank_receipt_asset_id: payment
            .bank_receipt_asset_id
            .ok_or_else(|| Error::BusinessLogicError("充值缺少银行回单".into()))?
            .to_string(),
        external_transaction_id: command.external_transaction_id,
        external_evidence_reference: command.evidence_reference,
        received_at: command.received_at,
    };
    let result = SupplierDepositService::new(db.clone())
        .receive_recharge(&account, &command.operation_key, request.amount, evidence, actor.id(), executor)
        .await?;
    audit(&db, &actor, "receive_recharge", &account, executor).await?;
    Ok(result)
}
