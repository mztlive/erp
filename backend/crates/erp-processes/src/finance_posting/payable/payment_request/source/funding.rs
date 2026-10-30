//! 冻结付款方式对应的真实账户，不用余额充足代替付款审批。

use erp_finance::entity::payable::{
    PaymentRequestAccountSnapshot, PaymentRequestDepositAccountSnapshot, PaymentRequestExecutionMethod,
    PurchasePaymentRequest,
};
use erp_finance::entity::supplier_deposit::DepositCurrency;
use erp_finance::repository::SupplierDepositExt;
use mongodb::Database;
use persistence_core::Executor;

use super::selected_account;
use crate::{Error, Result};

pub(super) async fn selected_accounts(
    db: &Database,
    request: &PurchasePaymentRequest,
    payer_party_id: &str,
    lock: bool,
    executor: &mut dyn Executor,
) -> Result<(Option<PaymentRequestAccountSnapshot>, Option<PaymentRequestDepositAccountSnapshot>)> {
    match &request.data.execution_method {
        PaymentRequestExecutionMethod::BankTransfer => {
            Ok((Some(selected_account(db, request, lock, executor).await?), None))
        },
        PaymentRequestExecutionMethod::SupplierDeposit { account_id } => {
            let account = db
                .supplier_deposit_accounts()
                .find_by_id(account_id, executor)
                .await?
                .ok_or_else(|| Error::NotFound("预存款账户不存在".into()))?;
            if !account.enabled
                || account.identity.supplier_id != request.data.supplier_id
                || account.identity.payer_party_id != payer_party_id
                || account.identity.currency != DepositCurrency::CNY
            {
                return Err(Error::ConflictError("预存款账户停用、主体、供应商或币种不符合本次申请".into()));
            }
            // 余额及资金对账限制在执行占用时重验，审批本身不登记资金使用。
            Ok((
                None,
                Some(PaymentRequestDepositAccountSnapshot {
                    account_id: account.base.id,
                    identity: account.identity,
                }),
            ))
        },
    }
}

pub(super) async fn revalidate(
    db: &Database,
    request: &PurchasePaymentRequest,
    executor: &mut dyn Executor,
) -> Result<()> {
    let (bank, deposit) = selected_accounts(db, request, &request.payer_party_id, true, executor).await?;
    if bank != request.payee_account || deposit != request.deposit_account {
        return Err(Error::ConflictError("批准付款账户或归属已变化，请处理原未付部分并重新申请".into()));
    }
    Ok(())
}
