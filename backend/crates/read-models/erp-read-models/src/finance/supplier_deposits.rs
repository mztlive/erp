//! 已获主体和供应商范围授权后的预存款明细与业务名称投影。

mod records;

use erp_core::common::time::BusinessDate;
use erp_core::money::Amount;
use erp_finance::entity::supplier_deposit::{
    SupplierDepositAccount, SupplierDepositAuthorization, SupplierDepositBatch, SupplierDepositLedgerEntry,
    SupplierDepositRechargeRequest, SupplierDepositReconciliation, SupplierDepositReservation,
    SupplierDepositUsage,
};
use erp_finance::repository::SupplierDepositExt;
use erp_finance::repository::supplier_deposit_query::latest_account_records as latest;
use erp_finance::repository::supplier_deposit_resolution::{
    outstanding_reservations, unresolved_reconciliations,
};
use erp_party::PartyExt;
use erp_party::repository::prelude::PartyBankAccountRepositoryExt;
use erp_supplier::SupplierExt;
use mongodb::Database;
use persistence_core::Executor;
use serde::Serialize;

use self::records::merge_recent;
use crate::{Error, Result};

#[derive(Debug, Clone, Serialize)]
pub struct DepositAccountRow {
    pub account: SupplierDepositAccount,
    pub payer_party_name: String,
    pub supplier_name: String,
    pub external_account_label: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct DepositPayeeBankAccount {
    pub id: String,
    pub account_name: String,
    pub bank_name: String,
    pub account_number_masked: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct DepositAccountDetail {
    #[serde(flatten)]
    pub row: DepositAccountRow,
    pub available_amount: Amount,
    pub batches: Vec<SupplierDepositBatch>,
    pub reservations: Vec<SupplierDepositReservation>,
    pub usages: Vec<SupplierDepositUsage>,
    pub ledger: Vec<SupplierDepositLedgerEntry>,
    pub authorizations: Vec<SupplierDepositAuthorization>,
    pub reconciliations: Vec<SupplierDepositReconciliation>,
    pub recharge_requests: Vec<SupplierDepositRechargeRequest>,
    pub payee_bank_accounts: Vec<DepositPayeeBankAccount>,
}

/// 为已获得账户范围权限的记录装配业务名称，不输出技术账号明文。
/// # 参数
/// * `db` / `executor` - 原授权查询执行器。
/// * `account` - 服务端已验证范围的账户。
/// # 返回
/// 返回主体、供应商名称及安全账户说明。
/// # 错误
/// 归属主体或当前名称版本缺失时拒绝。
pub async fn account_row(
    db: &Database,
    account: SupplierDepositAccount,
    executor: &mut dyn Executor,
) -> Result<DepositAccountRow> {
    let payer_party_name = party_name(db, &account.identity.payer_party_id, executor).await?;
    let supplier = db
        .supplier_accounts()
        .find_by_id(&account.identity.supplier_id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("供应商不存在".into()))?;
    let supplier_name = party_name(db, supplier.party_id.as_ref(), executor).await?;
    let external_account_label = format!("{supplier_name}预存款（人民币）");
    Ok(DepositAccountRow { account, payer_party_name, supplier_name, external_account_label })
}

/// 返回当前账户近期业务证据及所有待处理占用和对账差异。
/// # 参数
/// * `db` / `executor` - 已授权的同一查询执行器。
/// * `account` - 已完成主体/供应商范围核验的资金账户。
/// # 返回
/// 各类返回最近一百项；占用和对账额外包含全部未处理项，最多各一千项。
/// 同时返回明确的资金分类和供应商脱敏银行账户候选。
/// # 错误
/// 缺失归属、待处理项超限或查询失败时拒绝，不能静默遗漏历史资金。
pub async fn account_detail(
    db: &Database,
    account: SupplierDepositAccount,
    executor: &mut dyn Executor,
) -> Result<DepositAccountDetail> {
    let id = account.base.id.clone();
    let available_amount = account.available()?;
    let row = account_row(db, account, executor).await?;
    let payee_bank_accounts = payee_bank_accounts(db, &row.account.identity.supplier_id, executor).await?;

    let reservations = merge_recent(
        latest(db.supplier_deposit_reservations(), &id, executor).await?,
        outstanding_reservations(db, &id, executor).await?,
    )?;
    let reconciliations = merge_recent(
        latest(db.supplier_deposit_reconciliations(), &id, executor).await?,
        unresolved_reconciliations(db, &id, executor).await?,
    )?;

    Ok(DepositAccountDetail {
        row,
        available_amount,
        batches: latest(db.supplier_deposit_batches(), &id, executor).await?,
        reservations,
        usages: latest(db.supplier_deposit_usages(), &id, executor).await?,
        ledger: latest(db.supplier_deposit_ledger(), &id, executor).await?,
        authorizations: latest(db.supplier_deposit_authorizations(), &id, executor).await?,
        reconciliations,
        recharge_requests: latest(db.supplier_deposit_recharge_requests(), &id, executor).await?,
        payee_bank_accounts,
    })
}

async fn payee_bank_accounts(
    db: &Database,
    supplier_id: &str,
    executor: &mut dyn Executor,
) -> Result<Vec<DepositPayeeBankAccount>> {
    let supplier = db
        .supplier_accounts()
        .find_by_id(supplier_id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("供应商不存在".into()))?;
    let banks =
        db.party_bank_accounts().list_active_on(&supplier.party_id, BusinessDate::today(), executor).await?;
    Ok(banks
        .into_iter()
        .map(|bank| DepositPayeeBankAccount {
            id: bank.base.id,
            account_name: bank.account_name,
            bank_name: bank.bank_name,
            account_number_masked: format!("********{}", bank.account_number_last4),
        })
        .collect())
}

async fn party_name(db: &Database, id: &str, executor: &mut dyn Executor) -> Result<String> {
    let party = db
        .parties()
        .find_by_id(id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("企业主体不存在".into()))?;
    let revision =
        party.stable.current_revision_id.ok_or_else(|| Error::NotFound("企业主体缺少当前名称版本".into()))?;
    Ok(db
        .party_revisions()
        .find_by_id(&revision, executor)
        .await?
        .ok_or_else(|| Error::NotFound("企业主体名称版本不存在".into()))?
        .legal_name)
}
