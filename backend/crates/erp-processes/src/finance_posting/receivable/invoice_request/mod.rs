//! 开票申请正式用例；应收写锁串行化提交、撤回、审批与开票额度变更。
mod cancel;
mod command;
mod execution;
mod submit;
pub(crate) use cancel::cancel;
use erp_core::money::Amount;
use erp_finance::entity::receivable::{ReceivableAccount, SalesInvoiceRequest};
use erp_finance::repository::ReceivableExt;
use erp_finance::repository::prelude::*;
pub(crate) use execution::*;
use mongodb::Database;
use persistence_core::Executor;

use crate::{Error, Result};

/// 加载指定申请，不存在时返回可处理的业务错误。
async fn load(db: &Database, id: &str, executor: &mut dyn Executor) -> Result<SalesInvoiceRequest> {
    db.sales_invoice_requests()
        .find_by_id(id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("开票申请不存在".into()))
}
/// 写锁应收并加载占用额度；所有竞争写入在同一账户上产生写冲突。
/// # 错误
/// 应收缺失或 CAS 写入失败时整笔业务事务回滚。
async fn lock_account(db: &Database, id: &str, executor: &mut dyn Executor) -> Result<ReceivableAccount> {
    let mut account = db
        .receivable_accounts()
        .find_by_id(id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("销售应收不存在".into()))?;
    db.receivable_accounts().update(&mut account, executor).await?;
    Ok(account)
}
/// 查询在审及已批准未开的占用额度，不读取历史已完成申请。
async fn reserved(db: &Database, account_id: &str, executor: &mut dyn Executor) -> Result<Amount> {
    Ok(db
        .sales_invoice_requests()
        .reserved_for_account(account_id, executor)
        .await?
        .iter()
        .fold(Amount::zero(), |sum, request| sum.checked_add(request.reserved())))
}
/// 已占用的申请额度不得超过当前可开票金额。
///
/// # 参数
/// * `db` - 数据库。
/// * `account` - 当前应收子账，以其 `open_invoiceable_total` 为上限。
/// * `executor` - 调用方执行器。
///
/// # 返回
/// 占用不超过可开票额度时无返回值。
///
/// # 错误
/// 占用额度读取失败时返回该错误；已占用额度大于 `open_invoiceable_total` 时返回冲突。
pub(crate) async fn ensure_reserved_capacity(
    db: &Database,
    account: &ReceivableAccount,
    executor: &mut dyn Executor,
) -> Result<()> {
    if reserved(db, &account.base.id, executor).await? > account.open_invoiceable_total {
        return Err(Error::ConflictError(
            "销售变更后的可开票金额不足以覆盖已有申请，请先处理占用额度的开票申请".into(),
        ));
    }
    Ok(())
}
