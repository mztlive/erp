//! 财务进项发票分配查询与事务内事实写入。
use erp_core::ids::{InvoiceId, PartyId, PayableAccountId, PurchaseInvoiceAllocationId};
use id_generator::next_id;
use mongodb::Database;
use persistence_core::Executor;

use super::PayableService;
use crate::dto::payable::{PurchaseInvoiceRegisteredView, RegisterPurchaseInvoiceRequest};
use crate::entity::payable::{
    PayableAccount, PurchaseInvoiceAllocation, PurchaseInvoiceAllocationLine, PurchaseInvoiceAllocationPlan,
};
use crate::entity::receivable::{Invoice, InvoiceData, InvoiceDirection, InvoiceKind};
use crate::repository::prelude::*;
use crate::repository::{PayableExt, ReceivableExt};
use crate::{Error, Result};
impl PayableService {
    /// 按已提交发票主键回读稳定登记结果。
    ///
    /// # 参数
    /// * `invoice_id` - 首次命令收据记录的进项发票主键
    /// * `executor` - 调用方授权读取快照的执行器
    ///
    /// # 返回
    /// 返回发票号码、金额和正式分配行。
    ///
    /// # 错误
    /// 收据引用损坏、发票或分配查询失败时返回错误。
    pub async fn purchase_invoice_registered_view(
        &self,
        invoice_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<PurchaseInvoiceRegisteredView> {
        let invoice_id = InvoiceId::new(invoice_id);
        let invoice = self
            .db
            .invoices()
            .find_by_id(&invoice_id, executor)
            .await?
            .ok_or_else(|| Error::Internal("进项发票命令收据引用不存在".to_string()))?;
        let allocations = self
            .db
            .purchase_invoice_allocations()
            .find_allocations_by_invoices(std::slice::from_ref(&invoice_id), executor)
            .await?;
        let views = allocations.iter().map(purchase_invoice_allocation_view).collect();
        Ok(PurchaseInvoiceRegisteredView {
            invoice_id: invoice_id.to_string(),
            invoice_no: invoice.invoice_no.ok_or_else(|| Error::Internal("进项发票缺少号码".into()))?,
            gross_amount: invoice.gross_amount,
            allocations: views,
        })
    }
}
/// 装配进项发票分配视图。
///
/// # 参数
/// * `allocation` - 进项发票分配实体
///
/// # 返回
/// 返回响应视图。
///
/// # 错误
/// 不返回错误。
pub(super) fn purchase_invoice_allocation_view(
    allocation: &PurchaseInvoiceAllocation,
) -> crate::dto::payable::PurchaseInvoiceAllocationView {
    crate::dto::payable::PurchaseInvoiceAllocationView {
        id: allocation.base.id.clone(),
        invoice_id: allocation.invoice_id.to_string(),
        allocation_seq: allocation.allocation_seq,
        allocation_action: allocation.allocation_action,
        payable_account_id: allocation.payable_account_id.to_string(),
        allocated_gross_amount: allocation.allocated_gross_amount,
        allocated_net_amount: allocation.allocated_net_amount,
        allocated_tax_amount: allocation.allocated_tax_amount,
        reverses_allocation_id: allocation.reverses_allocation_id.as_ref().map(|id| id.to_string()),
    }
}

/// 为供应商主体事实构建进项发票；调用者先完成命令幂等和供应商存在校验。
///
/// # 参数
/// * `req` - 进项发票登记请求。
/// * `party_id` - 已确认的供应商往来主体。
/// * `actor_id` - 登记人。
///
/// # 返回
/// 返回方向为进项、种类为蓝字、尚未登记状态迁移的发票。舍入调整额为零。
///
/// # 错误
/// 发票实体构造失败时返回对应错误。
pub fn prepare_purchase_invoice(
    req: &RegisterPurchaseInvoiceRequest,
    party_id: PartyId,
    actor_id: &str,
) -> Result<Invoice> {
    let invoice_id = InvoiceId::new(next_id());
    let invoice = Invoice::new(
        invoice_id,
        InvoiceData {
            invoice_direction: InvoiceDirection::Purchase,
            invoice_kind: InvoiceKind::Blue,
            party_id,
            invoice_code: req.invoice_code.clone(),
            invoice_no: req.invoice_no.clone(),
            invoice_date: req.invoice_date,
            gross_amount: req.gross_amount,
            net_amount: req.net_amount,
            tax_amount: req.tax_amount,
            rounding_adjustment_amount: erp_core::money::Amount::zero(),
            rounding_reason: None,
            original_invoice_id: None,
        },
        actor_id,
    )?;
    Ok(invoice)
}
/// 在调用方 Executor 内按原顺序校验号码、构建分配计划并批量读取应付账户。
/// 供应商主体一致性由流程使用这些财务账户事实完成，之后才可调用持久化接口。
///
/// # 参数
/// * `db` - 财务领域数据库。
/// * `req` - 含分配行的登记请求。
/// * `invoice_for_tx` - 已构造、尚未持久化的进项发票。
/// * `session` - 调用方事务执行器。
///
/// # 返回
/// 返回分配计划，以及计划涉及的应付子账；本函数不校验账户是否齐全。
///
/// # 错误
/// 同方向规范化号码已登记时返回 `ConflictError`；分配计划构造失败或账户查询失败时返回对应错误。
pub async fn prepare_purchase_invoice_allocations(
    db: &Database,
    req: &RegisterPurchaseInvoiceRequest,
    invoice_for_tx: &Invoice,
    session: &mut dyn Executor,
) -> Result<(PurchaseInvoiceAllocationPlan, Vec<PayableAccount>)> {
    if db
        .invoices()
        .find_by_direction_and_normalized_no(
            InvoiceDirection::Purchase,
            &invoice_for_tx.normalized_no,
            session,
        )
        .await?
        .is_some()
    {
        return Err(Error::ConflictError("发票号码已登记，请勿重复提交".to_string()));
    }

    let lines: Vec<PurchaseInvoiceAllocationLine> = req
        .allocations
        .iter()
        .map(|line| PurchaseInvoiceAllocationLine {
            payable_account_id: line.payable_account_id.clone(),
            allocated_gross_amount: line.allocated_gross_amount,
            allocated_net_amount: line.allocated_net_amount,
            allocated_tax_amount: line.allocated_tax_amount,
        })
        .collect();
    let allocation_ids: Vec<PurchaseInvoiceAllocationId> =
        (0..lines.len()).map(|_| PurchaseInvoiceAllocationId::new(next_id())).collect();
    let (net, tax) = invoice_for_tx.tax_breakdown()?;
    let plan = PurchaseInvoiceAllocationPlan::new(
        invoice_for_tx.base.id.clone().into(),
        invoice_for_tx.gross_amount,
        net,
        tax,
        &lines,
        &allocation_ids,
    )?;

    // 账户/供应商事实一次批量装载；按行首次出现顺序逐账户校验，
    // 保持原逐行首错语义（存在性 → 供应商 → 跨供应商主体一致）。
    let account_ids: Vec<PayableAccountId> =
        plan.account_invoicing_deltas().iter().map(|(account_id, _)| account_id.clone()).collect();
    let accounts = db.payable_accounts().find_accounts_by_ids(&account_ids, session).await?;
    Ok((plan, accounts))
}
/// 使用原 Executor 更新收票额度、发票状态与分配事实，禁止在此写工作项或审计。
///
/// # 参数
/// * `db` - 财务领域数据库。
/// * `invoice_for_tx` - 待登记的进项发票。
/// * `plan` - 已校验的分配计划。
/// * `actor_id` - 登记人。
/// * `session` - 调用方事务执行器。
///
/// # 返回
/// 返回已标记登记并完成持久化的发票。
///
/// # 错误
/// 任一子账剩余可收票额度不足时返回 `BusinessLogicError`，且不写发票与分配；
/// 状态迁移或仓储写入失败时返回对应错误。
pub async fn persist_purchase_invoice(
    db: &Database,
    invoice_for_tx: Invoice,
    plan: &PurchaseInvoiceAllocationPlan,
    actor_id: &str,
    session: &mut dyn Executor,
) -> Result<Invoice> {
    let invoicing = db
        .payable_accounts()
        .apply_invoicings_many(plan.account_invoicing_deltas(), actor_id, session)
        .await?;
    if !invoicing.rejected.is_empty() {
        return Err(Error::BusinessLogicError("子账剩余可收票额度不足，收票被拒绝".to_string()));
    }
    let mut invoice_mut = invoice_for_tx;
    invoice_mut.mark_registered(actor_id)?;
    db.invoices().create(&invoice_mut, session).await?;
    db.payable().create_purchase_invoice_allocations_many(plan.new_allocations(), session).await?;
    Ok(invoice_mut)
}

#[cfg(test)]
mod purchase_invoice_allocation_list_tests {
    use std::str::FromStr;

    use erp_core::ids::{InvoiceId, PayableAccountId, PurchaseInvoiceAllocationId};
    use erp_core::money::Amount;

    use super::purchase_invoice_allocation_view;
    use crate::entity::payable::{
        AllocationAction, PurchaseInvoiceAllocation, PurchaseInvoiceAllocationData,
    };

    /// 构造具有指定稳定 ID 与秒级创建时间的最小进项发票分配事实。
    ///
    /// 参数提供排序键，返回通过实体校验的正式分配；测试金额固定为 `1.00`，
    /// 构造失败时直接 panic，且不访问数据库。
    fn allocation(id: &str, created_at: u64) -> PurchaseInvoiceAllocation {
        let mut allocation = PurchaseInvoiceAllocation::new(
            PurchaseInvoiceAllocationId::new(id),
            PurchaseInvoiceAllocationData {
                invoice_id: InvoiceId::new("invoice-1"),
                payable_account_id: PayableAccountId::new("account-1"),
                allocation_seq: 1,
                allocation_action: AllocationAction::Apply,
                allocated_gross_amount: Amount::from_str("1.00").unwrap(),
                allocated_net_amount: Amount::from_str("1.00").unwrap(),
                allocated_tax_amount: Amount::from_str("0.00").unwrap(),
                reverses_allocation_id: None,
            },
        )
        .unwrap();
        allocation.base.created_at = created_at;
        allocation
    }

    /// 视图映射保留分配身份，不改变金额精度。
    #[test]
    fn allocation_view_mapping_preserves_identity() {
        let view = purchase_invoice_allocation_view(&allocation("a-1", 10));
        assert_eq!(view.id, "a-1");
        assert_eq!(view.invoice_id, "invoice-1");
        assert_eq!(view.payable_account_id, "account-1");
    }
}
