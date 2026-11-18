//! 财务账号、独立充值审批与有限额执行授权。

use erp_core::common::time::{BusinessDate, Instant};
use erp_core::ids::SupplierPaymentId;
use erp_core::money::Amount;
use id_generator::{DocumentNumberGenerator, DocumentNumberKind};
use persistence_core::Executor;

use super::{SupplierDepositService, fingerprint, identity, replay, require_transaction};
use crate::entity::payable::{SupplierPayment, SupplierPaymentData, SupplierPaymentPurpose};
use crate::entity::supplier_deposit::{
    DepositAccountIdentity, DepositRechargeStatus, SupplierDepositAccount, SupplierDepositAuthorization,
    SupplierDepositRechargeRequest, SupplierDepositReconciliation,
};
use crate::repository::supplier_deposit_query::{bank_source_exists, ledger_balance_at};
use crate::repository::{PayableExt, SupplierDepositExt};
use crate::{Error, Result};

/// 原批准充值申请及所见版本，三者共同定位本次真实银行付款的批准依据。
pub struct DepositRechargePaymentTarget<'a> {
    pub account_id: &'a str,
    pub request_id: &'a str,
    pub expected_version: u64,
}

impl SupplierDepositService {
    /// 创建空余额资金账户，禁止以外部余额初始化。
    /// 返回原账户或新账户。
    /// 重放归属变化、非法身份或唯一冲突时拒绝。
    ///
    /// 输入约定：
    /// - `operation_key` / `identity_data`：稳定命令及已核验归属。
    /// - `executor`：账户和审计共同事务。
    pub async fn open_account(
        &self,
        operation_key: &str,
        identity_data: DepositAccountIdentity,
        executor: &mut dyn Executor,
    ) -> Result<SupplierDepositAccount> {
        require_transaction(executor)?;
        let normalized = identity_data.normalize()?;
        let id = identity("account", operation_key)?;
        if let Some(existing) = self.db.supplier_deposit_accounts().find_by_id(&id, executor).await? {
            if existing.identity != normalized {
                return Err(Error::ConflictError("同一开户操作不能更换资金归属".into()));
            }
            return Ok(existing);
        }

        let account = SupplierDepositAccount::new(id, normalized)?;
        self.db.supplier_deposit_accounts().create(&account, executor).await?;
        Ok(account)
    }

    /// 登记尚不可用的充值申请；申请批准不代表资金到账。
    /// 返回原申请或新待批准申请。
    /// 金额、账户或重复载荷非法时拒绝。
    ///
    /// 输入约定：
    /// - `account` / `operation`：账户与命令身份。
    /// - `amount` / `reason` / `actor`：金额、用途和申请人。
    /// - `executor`：当前事务。
    pub async fn request_recharge(
        &self,
        account: &str,
        operation: &str,
        amount: Amount,
        reason: String,
        actor: &str,
        executor: &mut dyn Executor,
    ) -> Result<SupplierDepositRechargeRequest> {
        require_transaction(executor)?;
        self.account(account, executor).await?;
        let id = identity(account, &format!("recharge:{operation}"))?;
        let payload = fingerprint(&(account, amount, &reason))?;
        if let Some(existing) = self.db.supplier_deposit_recharge_requests().find_by_id(&id, executor).await?
        {
            replay(&payload, &existing.payload_fingerprint)?;
            return Ok(existing);
        }

        let mut request = SupplierDepositRechargeRequest::new(
            id,
            account.into(),
            operation.into(),
            payload,
            amount,
            reason,
            actor.into(),
        )?;
        request.request_no = DocumentNumberGenerator::new(self.db.clone())
            .next_number(
                DocumentNumberKind::SupplierDepositRecharge,
                BusinessDate::today().as_naive_date(),
                executor,
            )
            .await
            .map_err(|error| Error::Internal(error.to_string()))?;
        self.db.supplier_deposit_recharge_requests().create(&request, executor).await?;
        Ok(request)
    }

    /// 由独立财务人员明确批准充值申请。
    /// 返回尚未付款的已批准充值申请。
    /// 状态、版本或独立批准资格不符时拒绝。
    ///
    /// 输入约定：
    /// - `account` / `id` / `version`：原归属、申请和所见版本。
    /// - `reference` / `actor`：批准依据及批准人。
    /// - `executor`：同一批准事务。
    pub async fn approve_recharge(
        &self,
        account: &str,
        id: &str,
        version: u64,
        reference: String,
        actor: &str,
        executor: &mut dyn Executor,
    ) -> Result<SupplierDepositRechargeRequest> {
        require_transaction(executor)?;
        let mut request = self.recharge_request(account, id, executor).await?;
        if request.approved_by.as_deref() == Some(actor)
            && request.approval_reference.as_deref() == Some(reference.as_str())
        {
            return Ok(request);
        }

        ensure_version(request.base.version, version)?;
        request.approve(actor, reference, Instant::now())?;
        self.db.supplier_deposit_recharge_requests().update(&mut request, executor).await?;
        Ok(request)
    }

    /// 登记已批准充值的真实银行付款一次，不核销任何采购应付。
    /// 返回银行已付、供应商到账仍待核实的申请。
    /// 未批准、金额或主体不符、重复付款或版本漂移时拒绝。
    ///
    /// 输入约定：
    /// - `target`：原账户、充值申请和所见版本的共同定位。
    /// - `data`：组合层已核验收款账户与回单的实际付款数据。
    /// - `payer_name` / `actor`：已核验主体名称及执行人。
    /// - `executor`：申请、银行付款和审计共同事务。
    pub async fn register_recharge_payment(
        &self,
        target: DepositRechargePaymentTarget<'_>,
        data: SupplierPaymentData,
        payer_name: &str,
        actor: &str,
        executor: &mut dyn Executor,
    ) -> Result<SupplierDepositRechargeRequest> {
        let DepositRechargePaymentTarget { account_id: account, request_id: id, expected_version: version } =
            target;
        require_transaction(executor)?;
        let mut request = self.recharge_request(account, id, executor).await?;
        if let Some(payment_id) = &request.supplier_payment_id {
            self.verify_existing_recharge_payment(payment_id, &data, actor, executor).await?;
            return Ok(request);
        }

        ensure_version(request.base.version, version)?;
        let source = self.account(account, executor).await?;
        if let Some(reference) = data.bank_reference.as_deref()
            && bank_source_exists(&self.db, &source.identity.payer_party_id, reference.trim(), executor)
                .await?
        {
            return Err(Error::ConflictError("该银行流水已形成付款，不能再次登记充值现金来源".into()));
        }
        if request.status != DepositRechargeStatus::Approved
            || data.amount != request.amount
            || data.supplier_id.as_ref() != source.identity.supplier_id
        {
            return Err(Error::BusinessLogicError("充值未批准或付款金额、供应商不符".into()));
        }

        let payment_id = identity(account, &format!("bank-payment:{id}"))?;
        let mut payment = SupplierPayment::new(SupplierPaymentId::new(payment_id), data, actor)?;
        payment.post_deposit_recharge(
            SupplierPaymentPurpose::SupplierDepositRecharge {
                recharge_request_id: id.into(),
                deposit_account_id: account.into(),
                approval_reference: request
                    .approval_reference
                    .clone()
                    .ok_or_else(|| Error::Internal("已批准充值缺少审批依据".into()))?,
            },
            &source.identity.payer_party_id,
            payer_name,
            actor,
            Instant::now(),
        )?;

        request.supplier_payment_id = Some(payment.base.id.clone());
        request.status = DepositRechargeStatus::BankPaid;
        self.db.supplier_payments().create(&payment, executor).await?;
        self.db.supplier_deposit_recharge_requests().update(&mut request, executor).await?;
        Ok(request)
    }

    async fn verify_existing_recharge_payment(
        &self,
        payment_id: &str,
        data: &SupplierPaymentData,
        actor: &str,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let payment = self
            .db
            .supplier_payments()
            .find_by_id(payment_id, executor)
            .await?
            .ok_or_else(|| Error::Internal("原充值付款记录缺失".into()))?;
        if payment.paid_at == data.paid_at
            && payment.payee_bank_account_id.as_ref() == Some(&data.payee_bank_account_id)
            && payment.amount == data.amount
            && payment.bank_reference == data.bank_reference
            && payment.bank_receipt_asset_id.as_ref() == Some(&data.bank_receipt_asset_id)
            && payment.posted_by.as_deref() == Some(actor)
        {
            return Ok(());
        }
        Err(Error::ConflictError("该充值申请已登记另一笔银行付款，请查询原结果".into()))
    }

    pub(super) async fn recharge_request(
        &self,
        account: &str,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<SupplierDepositRechargeRequest> {
        self.db
            .supplier_deposit_recharge_requests()
            .find_by_id(id, executor)
            .await?
            .filter(|request| request.account_id == account)
            .ok_or_else(|| Error::NotFound("充值申请不存在或不属于当前资金账户".into()))
    }

    /// 保存有期限、有单笔与累计限额的财务授权，不使用客户端指定授权人。
    /// 返回唯一已保存授权；同命令相同条款恢复原结果。
    /// 无效期限、限额、归属或重复异载荷时拒绝。
    ///
    /// 输入约定：
    /// - `authorization`：组合层装配实际授权人和版本的条款。
    /// - `executor`：当前事务。
    pub async fn authorize(
        &self,
        authorization: SupplierDepositAuthorization,
        executor: &mut dyn Executor,
    ) -> Result<SupplierDepositAuthorization> {
        require_transaction(executor)?;
        self.account(&authorization.account_id, executor).await?;
        if authorization.valid_from >= authorization.valid_until
            || authorization.per_execution_limit <= Amount::zero()
            || authorization.cumulative_limit < authorization.per_execution_limit
            || authorization.committed_amount != Amount::zero()
            || authorization.revoked
            || authorization.authorized_by.trim().is_empty()
            || authorization.approval_reference.trim().is_empty()
            || authorization.grant_version != 1
        {
            return Err(Error::ValidationError("财务授权须有完整身份、期限及有效限额".into()));
        }

        if let Some(existing) =
            self.db.supplier_deposit_authorizations().find_by_id(&authorization.base.id, executor).await?
        {
            if existing.mode != authorization.mode
                || existing.authorized_by != authorization.authorized_by
                || existing.account_id != authorization.account_id
                || existing.approval_reference != authorization.approval_reference
                || existing.valid_from != authorization.valid_from
                || existing.valid_until != authorization.valid_until
                || existing.per_execution_limit != authorization.per_execution_limit
                || existing.cumulative_limit != authorization.cumulative_limit
                || existing.purchase_order_ids != authorization.purchase_order_ids
            {
                return Err(Error::ConflictError("同一授权操作不能用于不同条款".into()));
            }
            return Ok(existing);
        }

        self.db.supplier_deposit_authorizations().create(&authorization, executor).await?;
        Ok(authorization)
    }

    /// 撤销新余额执行权限，已发送调查和已发生资金事实继续处理。
    /// 结果：授权标记撤销，不释放原占用。
    /// 越账户或版本漂移时拒绝。
    ///
    /// 输入约定：
    /// - `account` / `id` / `version`：原账户、授权和版本。
    /// - `executor`：当前财务事务。
    pub async fn revoke_authorization(
        &self,
        account: &str,
        id: &str,
        version: u64,
        executor: &mut dyn Executor,
    ) -> Result<SupplierDepositAuthorization> {
        require_transaction(executor)?;
        let mut authorization = self
            .db
            .supplier_deposit_authorizations()
            .find_by_id(id, executor)
            .await?
            .filter(|source| source.account_id == account)
            .ok_or_else(|| Error::NotFound("财务授权不存在".into()))?;
        if authorization.revoked {
            return Ok(authorization);
        }
        ensure_version(authorization.base.version, version)?;
        authorization.revoked = true;
        self.db.supplier_deposit_authorizations().update(&mut authorization, executor).await?;
        Ok(authorization)
    }

    /// 追加同一时间范围对账证据；差异限制新增使用，不覆盖本地余额。
    /// 返回原对账或新记录。
    /// 不完整时间/证据、重复异载荷或写入失败时拒绝。
    ///
    /// 输入约定：
    /// - `record`：已核验时间范围与来源证据。
    /// - `executor`：账户限制和对账记录共同事务。
    pub async fn record_reconciliation(
        &self,
        record: SupplierDepositReconciliation,
        executor: &mut dyn Executor,
    ) -> Result<SupplierDepositReconciliation> {
        require_transaction(executor)?;
        if record.covered_from >= record.covered_until
            || record.observed_at < record.covered_until
            || record.evidence_reference.trim().is_empty()
        {
            return Err(Error::ValidationError("对账时间范围或证据不完整".into()));
        }

        if let Some(existing) =
            self.db.supplier_deposit_reconciliations().find_by_id(&record.base.id, executor).await?
        {
            let mut candidate = record.clone();
            candidate.base = existing.base.clone();
            candidate.resolved_by = existing.resolved_by.clone();
            candidate.resolution_reference = existing.resolution_reference.clone();
            if fingerprint(&candidate)? != fingerprint(&existing)? {
                return Err(Error::ConflictError("同一对账操作不能用于不同证据".into()));
            }
            return Ok(existing);
        }

        let mut account = self.account(&record.account_id, executor).await?;
        account.retain_legacy_reconciliation_risk()?;
        let local_balance =
            ledger_balance_at(&self.db, &record.account_id, record.covered_until, executor).await?;
        if local_balance != record.local_balance_at_cutoff {
            return Err(Error::ConflictError("本地资金流水与填写的截止余额不一致，请重新核对".into()));
        }
        if record.remote_balance != record.local_balance_at_cutoff
            || !record.unresolved_transaction_ids.is_empty()
        {
            account.flag_reconciliation_risk(Instant::now())?;
        }

        self.db.supplier_deposit_accounts().update(&mut account, executor).await?;
        self.db.supplier_deposit_reconciliations().create(&record, executor).await?;
        Ok(record)
    }
}

fn ensure_version(actual: u64, expected: u64) -> Result<()> {
    if actual != expected {
        return Err(Error::ConflictError("财务记录已变化，请刷新后重新核对".into()));
    }
    Ok(())
}
