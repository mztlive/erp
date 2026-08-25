use std::str::FromStr;

use database::{FulfillmentExt, PayableExt, PurchaseOrderExt};
use entities::fulfillment::PurchaseFulfillmentEligibility;
use entities::ids::{PayableAccountId, PayableEntryId, PurchaseLineSalesAllocationId, PurchaseOrderId};
use entities::money::Amount;
use entities::payable::AllocationAction as PayableAllocationAction;
use entities::purchase_order::{PurchaseOrder, PurchaseOrderRevision};
use mongodb::Database;

use crate::errors::{Error, Result};

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
    PurchaseFulfillmentEligibility::ensure_order_fulfillable(po.stable.status)
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
    session: &mut mongodb::ClientSession,
    po: &PurchaseOrder,
) -> Result<()> {
    let revision = load_po_current_revision(db, session, po).await?;
    if !revision.payment_term_snapshot.prepay_gate {
        return Ok(());
    }
    let effective_paid = effective_paid_amount(db, session, &revision.purchase_order_id).await?;
    PurchaseFulfillmentEligibility::ensure_prepayment_satisfied(
        &revision.payment_term_snapshot,
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
    session: &mut mongodb::ClientSession,
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
    session: &mut mongodb::ClientSession,
    po_id: &PurchaseOrderId,
) -> Result<Amount> {
    let accounts = db
        .fulfillment()
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
    session: &mut mongodb::ClientSession,
    po: &PurchaseOrder,
    allocation_id: &PurchaseLineSalesAllocationId,
    sales_order_line_id: &entities::ids::SalesOrderLineId,
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
        .fulfillment()
        .sales_revision_line_for_allocation(
            &allocation.sales_order_revision_line_id,
            sales_order_line_id,
            session,
        )
        .await?;
    let sales_association = sales_revision_line.map(|line| {
        (
            entities::ids::SalesOrderRevisionLineId::new(line.base.id),
            line.sales_order_line_id,
        )
    });
    PurchaseFulfillmentEligibility::ensure_allocation_consistent(
        &allocation,
        &current_purchase_line_ids,
        sales_association
            .as_ref()
            .map(|(revision_line_id, stable_line_id)| (revision_line_id, stable_line_id)),
        sales_order_line_id,
    )
    .map_err(|error| Error::BusinessLogicError(error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::ensure_po_fulfillable;
    use entities::ids::{PurchaseOrderId, SalesOrderId, SupplierAccountId};
    use entities::purchase_order::{
        FulfillmentResponsibility, PurchaseOrder, PurchaseOrderData, PurchaseType,
    };

    #[test]
    fn po_fulfillable_guards_status() {
        let po = PurchaseOrder::new(
            PurchaseOrderId::new("po-1"),
            PurchaseOrderData {
                purchase_no: "PO-1".to_string(),
                sales_order_id: SalesOrderId::new("so-1"),
                sales_order_revision_id: entities::ids::SalesOrderRevisionId::new("sor-1"),
                creation_basis_id: "basis-1".to_string(),
                supplier_id: SupplierAccountId::new("sup-1"),
                purchase_type: PurchaseType::Physical,
                payment_term_code: "NET-30".to_string(),
                fulfillment_responsibility: FulfillmentResponsibility::Warehouse,
            },
            "admin-1",
        )
        .unwrap();
        assert!(ensure_po_fulfillable(&po).is_err(), "草稿不可履约");
    }
}
