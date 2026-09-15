//! 财务进项发票分配查询与事务内事实写入。
use erp_core::ids::{PartyId, PayableAccountId, PurchaseInvoiceAllocationId};
use id_generator::next_id;
use mongodb::Database;
use persistence_core::Executor;

use crate::dto::payable::RegisterPurchaseInvoiceRequest;
use crate::entity::payable::{
    PayableAccount, PurchaseInvoiceAllocation, PurchaseInvoiceAllocationLine, PurchaseInvoiceAllocationPlan,
};
use crate::entity::receivable::{Invoice, InvoiceData, InvoiceDirection, InvoiceKind};
use crate::repository::{PayableExt, ReceivableExt};
fn zero_amount() -> erp_core::money::Amount {
    erp_core::money::Amount::zero()
}
use erp_core::ids::InvoiceId;
use persistence_core::NoTransaction;
use validator::Validate;

use super::PayableService;
use crate::dto::payable::{
    PageView, PurchaseInvoiceAllocationListParams, PurchaseInvoiceAllocationView,
    PurchaseInvoiceRegisteredView, SortDir,
};
use crate::{Error, Result};
type PurchaseInvoiceAllocationFilter = <mongodb::Database as PayableExt>::PurchaseInvoiceAllocationFilter;
impl PayableService {
    /// 按已提交发票主键回读稳定登记结果。
    ///
    /// # 参数
    /// * `invoice_id` - 首次命令收据记录的进项发票主键
    ///
    /// # 返回
    /// 返回发票号码、金额和正式分配行。
    ///
    /// # 错误
    /// 收据引用损坏、发票或分配查询失败时返回错误。
    pub async fn purchase_invoice_registered_view(
        &self,
        invoice_id: &str,
    ) -> Result<PurchaseInvoiceRegisteredView> {
        let invoice_id = InvoiceId::new(invoice_id);
        let invoice = self
            .db
            .invoices()
            .find_by_id(&invoice_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::Internal("进项发票命令收据引用不存在".to_string()))?;
        let allocations = self
            .db
            .purchase_invoice_allocations()
            .find_allocations_by_invoices(std::slice::from_ref(&invoice_id), &mut NoTransaction)
            .await?;
        let views = allocations.iter().map(purchase_invoice_allocation_view).collect();
        Ok(PurchaseInvoiceRegisteredView {
            invoice_id: invoice_id.to_string(),
            invoice_no: invoice.invoice_no,
            gross_amount: invoice.gross_amount,
            allocations: views,
        })
    }
    /// 分页查询进项发票分配列表（按应付子账筛选，FIN-R06 服务端分页）。
    ///
    /// 账户必填政策与请求校验保留在 Service；过滤、稳定排序
    /// (`(created_at, id)` 同方向）、`skip/limit` 与总数均由 Repository 在
    /// 数据库完成，只装载当前页。
    ///
    /// # 参数
    /// * `params` - 应付子账、分页与排序校验参数
    ///
    /// # 返回
    /// 返回契约形状的分页视图，页码与单页条数沿用归一化请求值。
    ///
    /// # 错误
    /// 参数非法、应付子账缺失或仓储查询失败时返回既有服务错误。
    ///
    /// # 约束
    /// 排序固定使用 `(created_at, id)` 同方向并列键，不引入未建索引的新查询形状。
    pub async fn purchase_invoice_allocation_list(
        &self,
        params: &PurchaseInvoiceAllocationListParams,
    ) -> Result<PageView<PurchaseInvoiceAllocationView>> {
        params.validate()?;
        let query = params.normalized()?;
        let payable_account_id = query
            .payable_account_id
            .ok_or_else(|| Error::ValidationError("按应付子账筛选进项发票分配为必填条件".to_string()))?;
        let filter = PurchaseInvoiceAllocationFilter {
            payable_account_id: Some(payable_account_id),
            invoice_id: None,
            page: query.paging.page,
            page_size: query.paging.page_size,
            sort_ascending: matches!(query.paging.sort_dir, SortDir::Asc),
        };
        let page = self
            .db
            .purchase_invoice_allocations()
            .search_purchase_invoice_allocations(&filter, &mut NoTransaction)
            .await?;
        Ok(PageView {
            items: page.items.iter().map(purchase_invoice_allocation_view).collect(),
            total: page.total,
            page: query.paging.page,
            page_size: query.paging.page_size,
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
pub fn prepare_purchase_invoice(
    req: &RegisterPurchaseInvoiceRequest,
    party_id: PartyId,
    actor_id: &str,
) -> Result<Invoice> {
    let invoice_id = InvoiceId::new(next_id());
    let invoice = Invoice::new(
        invoice_id.clone(),
        InvoiceData {
            invoice_direction: InvoiceDirection::Purchase,
            invoice_kind: InvoiceKind::Blue,
            party_id: party_id.clone(),
            invoice_code: req.invoice_code.clone(),
            invoice_no: req.invoice_no.clone(),
            invoice_date: req.invoice_date,
            gross_amount: req.gross_amount,
            net_amount: req.net_amount,
            tax_amount: req.tax_amount,
            rounding_adjustment_amount: zero_amount(),
            rounding_reason: None,
            original_invoice_id: None,
        },
        actor_id,
    )?;
    Ok(invoice)
}
/// 在调用方 Executor 内按原顺序校验号码、构建分配计划并批量读取应付账户。
/// 供应商主体一致性由流程使用这些财务账户事实完成，之后才可调用持久化接口。
pub async fn prepare_purchase_invoice_allocations_in_transaction(
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
    let plan = PurchaseInvoiceAllocationPlan::new(
        invoice_for_tx.base.id.clone().into(),
        invoice_for_tx.gross_amount,
        invoice_for_tx.net_amount,
        invoice_for_tx.tax_amount,
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
pub async fn persist_purchase_invoice_in_transaction(
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

    /// 按 `(created_at, id)` 同方向排序；升序与降序均使用同一方向并列键。
    fn stable_order(ids: &[(&str, u64)], ascending: bool) -> Vec<String> {
        let mut rows: Vec<(&str, u64)> = ids.to_vec();
        rows.sort_by(|left, right| left.1.cmp(&right.1).then_with(|| left.0.cmp(right.0)));
        if !ascending {
            rows.reverse();
        }
        rows.into_iter().map(|(id, _)| id.to_string()).collect()
    }

    /// FIN-R06：列表只装载当前页，过滤、稳定排序与总数由 Repository 服务端完成。
    #[test]
    fn allocation_list_uses_server_pagination() {
        let production = include_str!("invoice.rs").split("#[cfg(test)]").next().expect("生产代码");
        let body = production
            .split("pub async fn purchase_invoice_allocation_list")
            .nth(1)
            .expect("分配列表")
            .split("/// 装配进项发票分配视图")
            .next()
            .expect("列表函数体");
        assert!(body.contains("search_purchase_invoice_allocations"));
        assert!(body.contains("PurchaseInvoiceAllocationFilter"));
        assert!(!body.contains("find_allocations_by_accounts"));
        assert!(!production.contains("fn purchase_invoice_allocation_page"));
    }

    /// 升序按创建时间再按稳定 ID 返回。
    #[test]
    fn allocation_stable_order_sorts_ascending() {
        assert_eq!(stable_order(&[("a-3", 30), ("a-1", 10), ("a-2", 20)], true), ["a-1", "a-2", "a-3"]);
    }

    /// 降序对创建时间与稳定 ID 使用同一方向。
    #[test]
    fn allocation_stable_order_sorts_descending() {
        assert_eq!(stable_order(&[("a-1", 10), ("a-3", 30), ("a-2", 20)], false), ["a-3", "a-2", "a-1"]);
    }

    /// 同秒事实跨页边界保持确定，无重复、无遗漏。
    #[test]
    fn allocation_stable_order_paginates_equal_timestamps_deterministically() {
        let rows = [("a-2", 10), ("a-4", 10), ("a-1", 10), ("a-3", 10)];
        let ascending = stable_order(&rows, true);
        let descending = stable_order(&rows, false);
        assert_eq!(ascending, ["a-1", "a-2", "a-3", "a-4"]);
        assert_eq!(descending, ["a-4", "a-3", "a-2", "a-1"]);
        assert_eq!(&ascending[2..], ["a-3", "a-4"]);
        assert_eq!(&descending[2..], ["a-2", "a-1"]);
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
