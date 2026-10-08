//! 采购首次正式化的原始应付账户与分录构造、事务内写入。
use std::str::FromStr;

use erp_core::common::time::{BusinessDate, Instant};
use erp_core::ids::{PayableEntryId, SupplierAccountId};
use erp_core::money::Amount;
use id_generator::next_id;
use persistence_core::Executor;

use crate::Result;
use crate::entity::payable::{PayableAccount, PayableEntry};
use crate::repository::PayableExt;
/// 采购原始应付消费事实；到期日由采购冻结付款条件计算。
pub struct InitialPurchasePayable<'a> {
    /// 采购单主键。
    pub order_id: &'a str,
    /// 冻结采购提交主键。
    pub submission_id: &'a str,
    /// 冻结供应商身份。
    pub supplier_id: SupplierAccountId,
    /// 原始含税应付金额。
    pub gross_amount: Amount,
    /// 按原付款条件冻结的到期业务日。
    pub due_date: BusinessDate,
}
/// 按账户、分录原次序分配身份并构造原始应付，保留原实体错误和发生时间。
///
/// # 参数
/// * `input` - 采购冻结的来源身份、供应商、含税金额与到期日。
/// * `actor_id` - 创建人。
///
/// # 返回
/// 返回来源为采购单的子账与序号为 1 的原始增加分录。已核销与已收票为零，可收票额等于含税合计。
///
/// # 错误
/// 子账或分录实体构造失败时返回对应错误。
pub fn prepare(input: InitialPurchasePayable<'_>, actor_id: &str) -> Result<(PayableAccount, PayableEntry)> {
    let account = crate::entity::payable::PayableAccount::new(
        erp_core::ids::PayableAccountId::new(next_id()),
        crate::entity::payable::PayableAccountData {
            source_document_id: input.order_id.to_string(),
            supplier_id: input.supplier_id,
            source_type: crate::entity::payable::PayableSourceType::PurchaseOrder,
            gross_total: input.gross_amount,
            settled_total: zero_amount(),
            invoiceable_total: input.gross_amount,
            invoiced_total: zero_amount(),
        },
        actor_id,
    )?;
    let entry = crate::entity::payable::PayableEntry::new(
        PayableEntryId::new(next_id()),
        crate::entity::payable::PayableEntryData {
            payable_account_id: account.base.id.clone().into(),
            entry_type: crate::entity::payable::PayableEntryType::Original,
            direction: crate::entity::payable::EntryDirection::Increase,
            amount: input.gross_amount,
            due_date: input.due_date,
            source_fact_type: "purchase_order".to_string(),
            source_document_id: input.order_id.to_string(),
            source_revision_id: input.submission_id.to_string(),
            source_sequence: 1,
            posted_at: Instant::now(),
        },
    )?;
    Ok((account, entry))
}
fn zero_amount() -> Amount {
    Amount::from_str("0").expect("零金额合法")
}
/// 将原始账户和分录按财务原子仓储合同写入调用方事务。
///
/// # 参数
/// * `db` - 财务领域数据库。
/// * `account` - 已构造的应付子账。
/// * `entry` - 已构造的原始分录。
/// * `executor` - 调用方事务执行器。
///
/// # 返回
/// 写入成功时无返回值。
///
/// # 错误
/// 仓储写入失败时返回对应错误。
pub async fn persist(
    db: &mongodb::Database,
    account: &PayableAccount,
    entry: &PayableEntry,
    executor: &mut dyn Executor,
) -> Result<()> {
    Ok(db.payable().create_payable_with_entry(account, entry, executor).await?)
}
