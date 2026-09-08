use std::str::FromStr;

use erp_core::ids::{PayableAccountId, PayableEntryId, PurchaseLineSalesAllocationId, PurchaseOrderId};
use erp_core::money::Amount;
use erp_finance::entity::payable::AllocationAction as PayableAllocationAction;
use erp_finance::repository::PayableExt;
use erp_fulfillment::entity::facts::{
    PrepaymentRequirementFact, PurchaseAllocationFact, PurchaseOrderStatusFact, PurchaseRevisionLineFact,
};
use erp_fulfillment::entity::fulfillment::PurchaseFulfillmentEligibility;
use erp_procurement::repository::PurchaseOrderExt;
use erp_sales::repository::SalesOrderExt;
use mongodb::Database;
use persistence_core::Executor;
use {
    erp_procurement::entity::purchase_order::PurchaseOrder,
    erp_procurement::entity::purchase_order::PurchaseOrderRevision,
};

use crate::{Error, Result};

/// 校验采购单处于可履约状态（§6.6：生效或部分执行）。
///
/// # 参数
/// * `po` - 采购单实体
///
/// # 返回
/// 可履约返回 `Ok(())`。
///
/// # 错误
/// 采购单不在生效/部分执行状态时返回 `BusinessLogicError`。
pub(super) fn ensure_po_fulfillable(po: &PurchaseOrder) -> Result<()> {
    PurchaseFulfillmentEligibility::ensure_order_fulfillable(match po.stable.status {
        erp_procurement::entity::purchase_order::PurchaseOrderStatus::Effective => {
            PurchaseOrderStatusFact::Effective
        }
        erp_procurement::entity::purchase_order::PurchaseOrderStatus::PartiallyExecuted => {
            PurchaseOrderStatusFact::PartiallyExecuted
        }
        _ => PurchaseOrderStatusFact::Other,
    })
    .map_err(|error| Error::BusinessLogicError(error.to_string()))
}

/// 校验 `PREPAY` 采购履约门槛（§8.1.5）。
///
/// 按采购单当前生效版本的付款条件快照判定是否先款后货；门槛开启时重算
/// 有效已过账付款净核销金额（D19：应付子账 → 分录 → 付款核销分配，`APPLY −
/// REVERSE` 净额），达到冻结的金额或比例门槛才允许过账。
///
/// # 参数
/// * `db` - 数据库实例
/// * `session` - 事务会话执行器
/// * `po` - 采购单实体
///
/// # 返回
/// 门槛满足返回 `Ok(())`。
///
/// # 错误
/// 生效版本缺失、或有效付款未达门槛时返回 `BusinessLogicError`。
pub(super) async fn ensure_prepay_gate(
    db: &Database,
    session: &mut dyn Executor,
    po: &PurchaseOrder,
) -> Result<()> {
    let revision = load_po_current_revision(db, session, po).await?;
    if !revision.payment_term_snapshot.prepay_gate {
        return Ok(());
    }
    let effective_paid = effective_paid_amount(db, session, &revision.purchase_order_id).await?;
    PurchaseFulfillmentEligibility::ensure_prepayment_satisfied(
        &PrepaymentRequirementFact {
            prepay_gate: revision.payment_term_snapshot.prepay_gate,
            prepay_minimum_amount: revision.payment_term_snapshot.prepay_minimum_amount,
            prepay_minimum_ratio: revision.payment_term_snapshot.prepay_minimum_ratio,
        },
        revision.gross_amount,
        effective_paid,
    )
    .map_err(|error| Error::BusinessLogicError(error.to_string()))
}

/// 取采购单当前生效版本。
///
/// # 参数
/// * `db` - 数据库实例
/// * `session` - 事务会话执行器
/// * `po` - 采购单实体
///
/// # 返回
/// 返回生效版本实体。
///
/// # 错误
/// 生效版本缺失时返回 `BusinessLogicError`。
pub(super) async fn load_po_current_revision(
    db: &Database,
    session: &mut dyn Executor,
    po: &PurchaseOrder,
) -> Result<PurchaseOrderRevision> {
    let revision_id = po
        .stable
        .current_revision_id
        .clone()
        .ok_or_else(|| Error::BusinessLogicError("采购单没有生效版本，无法履约".to_string()))?;
    db.purchase_order_revisions()
        .find_by_id(&revision_id, session)
        .await?
        .ok_or_else(|| Error::NotFound("采购生效版本不存在".to_string()))
}

/// 重算采购单的有效已过账付款净核销金额（D19 跨域只读）。
///
/// 路径：应付往来子账（来源单据 = 采购单）→ 应付分录 → 付款核销分配，
/// `APPLY − REVERSE` 净额求和。
///
/// # 参数
/// * `db` - 数据库实例
/// * `session` - 事务会话执行器
/// * `po_id` - 采购单
///
/// # 返回
/// 返回净核销金额（未付款为 0）。
///
/// # 错误
/// 任一步查询失败时返回 `RepositoryError`。
async fn effective_paid_amount(
    db: &Database,
    session: &mut dyn Executor,
    po_id: &PurchaseOrderId,
) -> Result<Amount> {
    let accounts = db
        .payable_accounts()
        .list_payable_accounts_for_purchase_order(po_id, session)
        .await?;
    let account_ids: Vec<PayableAccountId> = accounts
        .iter()
        .map(|account| account.base.id.clone().into())
        .collect();
    let entries = db
        .payable_entries()
        .find_entries_by_accounts(&account_ids, session)
        .await?;
    let entry_ids: Vec<PayableEntryId> = entries
        .iter()
        .filter(|entry| entry.source_document_id == po_id.to_string())
        .map(|entry| entry.base.id.clone().into())
        .collect();
    let allocations = db
        .payment_allocations()
        .find_allocations_by_entries(&entry_ids, session)
        .await?;
    let mut net = Amount::from_str("0").map_err(Error::Logic)?;
    for allocation in allocations {
        net = match allocation.allocation_action {
            PayableAllocationAction::Apply => net.checked_add(allocation.allocated_amount),
            PayableAllocationAction::Reverse => net.checked_sub(allocation.allocated_amount),
        };
    }
    Ok(net)
}

/// 校验采购销售分配有效（§6.7：采购行归属当前生效版本、销售行归属本明细）。
///
/// # 参数
/// * `db` - 数据库实例
/// * `session` - 事务会话执行器
/// * `po` - 采购单实体
/// * `allocation_id` - 采购销售分配
/// * `sales_order_line_id` - 销售稳定明细
///
/// # 返回
/// 有效返回 `Ok(())`。
///
/// # 错误
/// 分配不存在、采购行不属于当前生效版本或销售行不属于本明细时返回
/// `BusinessLogicError`。
pub(super) async fn ensure_allocation_valid(
    db: &Database,
    session: &mut dyn Executor,
    po: &PurchaseOrder,
    allocation_id: &PurchaseLineSalesAllocationId,
    sales_order_line_id: &erp_core::ids::SalesOrderLineId,
) -> Result<()> {
    let allocation = db
        .purchase_line_sales_allocations()
        .find_by_id(allocation_id.as_ref(), session)
        .await?
        .ok_or_else(|| Error::BusinessLogicError("采购销售分配不存在".to_string()))?;
    let revision = load_po_current_revision(db, session, po).await?;
    let revision_lines = db
        .purchase_order_revision_lines()
        .find_lines_by_revision_ids(&[revision.base.id.clone().into()], session)
        .await?;
    let current_purchase_line_ids = revision_lines
        .iter()
        .map(|line| line.base.id.clone().into())
        .collect::<Vec<_>>();
    let sales_revision_line = db
        .sales_order_revision_lines()
        .sales_revision_line_for_allocation(
            &allocation.sales_order_revision_line_id,
            sales_order_line_id,
            session,
        )
        .await?;
    let sales_association = sales_revision_line.map(|line| {
        (
            erp_core::ids::SalesOrderRevisionLineId::new(line.base.id),
            line.sales_order_line_id,
        )
    });
    PurchaseFulfillmentEligibility::ensure_allocation_consistent(
        &PurchaseAllocationFact {
            purchase_order_revision_line_id: allocation.purchase_order_revision_line_id.clone(),
            sales_order_revision_line_id: allocation.sales_order_revision_line_id.clone(),
        },
        &current_purchase_line_ids,
        sales_association
            .as_ref()
            .map(|(revision_line_id, stable_line_id)| (revision_line_id, stable_line_id)),
        sales_order_line_id,
    )
    .map_err(|error| Error::BusinessLogicError(error.to_string()))
}

/// 显式投影当前采购版本行的履约消费事实；不得提前检查后续收货行。
pub(super) fn revision_line_fact(
    line: &erp_procurement::entity::purchase_order::PurchaseOrderRevisionLine,
) -> PurchaseRevisionLineFact {
    PurchaseRevisionLineFact {
        id: line.base.id.clone().into(),
        quantity: line.quantity,
    }
}

#[cfg(test)]
mod tests {
    /// 将供应商受控付款规则映射为采购测试使用的消费事实。
    fn resolve_procurement_payment_term(
        code: &str,
    ) -> erp_core::Result<erp_procurement::entity::facts::PaymentTermFact> {
        let term = erp_supplier::SupplierPaymentTerm::parse(code)?;
        Ok(erp_procurement::entity::facts::PaymentTermFact {
            canonical_code: term.code().to_string(),
            prepay_gate: term.prepay_gate(),
            prepay_minimum_ratio: term.prepay_minimum_ratio(),
            days_after_delivery: term.days_after_delivery(),
        })
    }

    use super::ensure_po_fulfillable;
    use erp_core::ids::{PurchaseOrderId, SalesOrderId, SupplierAccountId};
    use {
        erp_procurement::entity::purchase_order::FulfillmentResponsibility,
        erp_procurement::entity::purchase_order::PurchaseOrder,
        erp_procurement::entity::purchase_order::PurchaseOrderData,
        erp_procurement::entity::purchase_order::PurchaseType,
    };

    #[test]
    fn po_fulfillable_guards_status() {
        let po = PurchaseOrder::new(
            PurchaseOrderId::new("po-1"),
            PurchaseOrderData {
                purchase_no: "PO-1".to_string(),
                sales_order_id: SalesOrderId::new("so-1"),
                sales_order_revision_id: erp_core::ids::SalesOrderRevisionId::new("sor-1"),
                creation_basis_id: "basis-1".to_string(),
                supplier_id: SupplierAccountId::new("sup-1"),
                purchase_type: PurchaseType::Physical,
                payment_term_code: "NET-30".to_string(),
                fulfillment_responsibility: FulfillmentResponsibility::Warehouse,
                owner_user_id: "buyer-1".to_string(),
                target_warehouse_id: Some(erp_core::ids::WarehouseId::new("wh-1")),
            },
            "admin-1",
            resolve_procurement_payment_term,
        )
        .unwrap();
        assert!(ensure_po_fulfillable(&po).is_err(), "草稿不可履约");
    }
}
