//! 演示清理只绑定领域公开的仓储，不接受外部集合名。
pub(in crate::demo_master_data) mod record;
use erp_contract::repository::ContractExt;
use erp_finance::repository::{CostExt, PayableExt, ReceivableExt};
use erp_fulfillment::repository::FulfillmentExt;
use erp_inventory::repository::InventoryExt;
use erp_procurement::repository::PurchaseOrderExt;
use erp_returns::repository::ReturnsExt;
use erp_sales::repository::{SalesOrderExt, SalesReviewExt, SalesSelectionExt};
use erp_supply::repository::{SupplierFulfillmentExt, SupplierOfferingExt, SupplierSettlementExt};
use erp_support::repository::FileAssetExt;
use erp_workflow::repository::{ApprovalIntegrationExt, BpmExt, DocumentRegistryExt, WorkItemExt};
use mongodb::Database;
use persistence_core::repository::IdRepository;

use crate::{Error, Result};

/// 按显式白名单选择拥有领域的仓储。
///
/// # 参数
/// `db` - 数据库；`collection` - 编排内部使用的集合标识。
///
/// # 返回
/// 返回绑定领域集合的 ID 仓储。
///
/// # 错误
/// 集合未在白名单登记时返回错误。
pub(super) fn ids(db: &Database, collection: &str) -> Result<IdRepository> {
    if let Some(repository) = workflow(db, collection) {
        return Ok(repository);
    }
    if let Some(repository) = contract(db, collection) {
        return Ok(repository);
    }
    if let Some(repository) = finance(db, collection) {
        return Ok(repository);
    }
    if let Some(repository) = fulfillment(db, collection) {
        return Ok(repository);
    }
    if let Some(repository) = returns(db, collection) {
        return Ok(repository);
    }
    if let Some(repository) = support(db, collection) {
        return Ok(repository);
    }
    if let Some(repository) = procurement(db, collection) {
        return Ok(repository);
    }
    if let Some(repository) = sales(db, collection) {
        return Ok(repository);
    }
    if let Some(repository) = inventory(db, collection) {
        return Ok(repository);
    }
    if let Some(repository) = supply(db, collection) {
        return Ok(repository);
    }
    Err(Error::Internal(format!("演示清理未登记集合：{collection}")))
}

/// 绑定 erp_workflow 拥有的清理集合。
fn workflow(db: &Database, collection: &str) -> Option<IdRepository> {
    Some(match collection {
        "approval_instance_assignees" => db.approval_instance_assignees().ids(),
        "approval_node_executions" => db.approval_node_executions().ids(),
        "approval_process_instances" => db.approval_process_instances().ids(),
        "approval_subject_snapshots" => db.approval_subject_snapshots().ids(),
        "business_documents" => db.business_documents().ids(),
        "document_participants" => db.document_participants().ids(),
        "document_relations" => db.document_relations().ids(),
        "work_items" => db.work_items().ids(),
        "workflow_actions" => db.workflow_actions().ids(),
        _ => return None,
    })
}

/// 绑定 erp_contract 拥有的清理集合。
fn contract(db: &Database, collection: &str) -> Option<IdRepository> {
    Some(match collection {
        "contract_revisions" => db.contract_revisions().ids(),
        "contracts" => db.contracts().ids(),
        _ => return None,
    })
}

/// 绑定 erp_finance 拥有的清理集合。
fn finance(db: &Database, collection: &str) -> Option<IdRepository> {
    Some(match collection {
        "cost_allocations" => db.cost_allocations().ids(),
        "customer_receipts" => db.customer_receipts().ids(),
        "invoices" => db.invoices().ids(),
        "payable_accounts" => db.payable_accounts().ids(),
        "payable_entries" => db.payable_entries().ids(),
        "payable_entry_offsets" => db.payable_entry_offsets().ids(),
        "payment_allocations" => db.payment_allocations().ids(),
        "purchase_invoice_allocations" => db.purchase_invoice_allocations().ids(),
        "receipt_allocations" => db.receipt_allocations().ids(),
        "receivable_accounts" => db.receivable_accounts().ids(),
        "receivable_entries" => db.receivable_entries().ids(),
        "receivable_entry_offsets" => db.receivable_entry_offsets().ids(),
        "sales_invoice_allocations" => db.sales_invoice_allocations().ids(),
        "sales_invoice_requests" => db.sales_invoice_requests().ids(),
        "supplier_payments" => db.supplier_payments().ids(),
        _ => return None,
    })
}

/// 绑定 erp_fulfillment 拥有的清理集合。
fn fulfillment(db: &Database, collection: &str) -> Option<IdRepository> {
    Some(match collection {
        "customer_acceptance_lines" => db.customer_acceptance_lines().ids(),
        "customer_acceptances" => db.customer_acceptances().ids(),
        "deliveries" => db.deliveries().ids(),
        "delivery_lines" => db.delivery_lines().ids(),
        "electronic_deliveries" => db.electronic_deliveries().ids(),
        "purchase_receipt_lines" => db.purchase_receipt_lines().ids(),
        "purchase_receipts" => db.purchase_receipts().ids(),
        "service_fulfillments" => db.service_fulfillments().ids(),
        _ => return None,
    })
}

/// 绑定 erp_returns 拥有的清理集合。
fn returns(db: &Database, collection: &str) -> Option<IdRepository> {
    Some(match collection {
        "customer_refunds" => db.customer_refunds().ids(),
        "purchase_return_lines" => db.purchase_return_lines().ids(),
        "purchase_return_orders" => db.purchase_return_orders().ids(),
        "sales_return_cases" => db.sales_return_cases().ids(),
        "sales_return_lines" => db.sales_return_lines().ids(),
        "supplier_refunds" => db.supplier_refunds().ids(),
        _ => return None,
    })
}

/// 绑定 erp_support 拥有的清理集合。
fn support(db: &Database, collection: &str) -> Option<IdRepository> {
    Some(match collection {
        "document_attachments" => db.document_attachments().ids(),
        _ => return None,
    })
}

/// 绑定 erp_procurement 拥有的清理集合。
fn procurement(db: &Database, collection: &str) -> Option<IdRepository> {
    Some(match collection {
        "purchase_change_orders" => db.purchase_change_orders().ids(),
        "purchase_change_submission_lines" => db.purchase_change_submission_lines().ids(),
        "purchase_change_submissions" => db.purchase_change_submissions().ids(),
        "purchase_line_sales_allocations" => db.purchase_line_sales_allocations().ids(),
        "purchase_order_revision_lines" => db.purchase_order_revision_lines().ids(),
        "purchase_order_revisions" => db.purchase_order_revisions().ids(),
        "purchase_order_submission_lines" => db.purchase_order_submission_lines().ids(),
        "purchase_order_submissions" => db.purchase_order_submissions().ids(),
        "purchase_orders" => db.purchase_orders().ids(),
        _ => return None,
    })
}

/// 绑定 erp_sales 拥有的清理集合。
fn sales(db: &Database, collection: &str) -> Option<IdRepository> {
    Some(match collection {
        "sales_change_orders" => db.sales_change_orders().ids(),
        "sales_change_submission_lines" => db.sales_change_submission_lines().ids(),
        "sales_change_submissions" => db.sales_change_submissions().ids(),
        "sales_order_goods_service_line_revisions" => db.sales_order_goods_service_line_revisions().ids(),
        "sales_order_lines" => db.sales_order_lines().ids(),
        "sales_order_revision_lines" => db.sales_order_revision_lines().ids(),
        "sales_order_revisions" => db.sales_order_revisions().ids(),
        "sales_order_submission_lines" => db.sales_order_submission_lines().ids(),
        "sales_order_submissions" => db.sales_order_submissions().ids(),
        "sales_order_voucher_line_revisions" => db.sales_order_voucher_line_revisions().ids(),
        "sales_order_working_copies" => db.sales_order_working_copies().ids(),
        "sales_order_working_copy_lines" => db.sales_order_working_copy_lines().ids(),
        "sales_orders" => db.sales_orders().ids(),
        "sales_selection_idempotency" => db.sales_selection_idempotency().ids(),
        "sales_selection_booklets" => db.sales_selection_booklets().ids(),
        "sales_selection_display_items" => db.sales_selection_display_items().ids(),
        "sales_selection_pool_members" => db.sales_selection_pool_members().ids(),
        "sales_selection_prepare_tasks" => db.sales_selection_prepare_tasks().ids(),
        "sales_selection_proposal_display_lines" => db.sales_selection_proposal_display_lines().ids(),
        "sales_selection_proposal_sku_lines" => db.sales_selection_proposal_sku_lines().ids(),
        "sales_selection_proposals" => db.sales_selection_proposals().ids(),
        "sales_selection_sessions" => db.sales_selection_sessions().ids(),
        _ => return None,
    })
}

/// 绑定 erp_inventory 拥有的清理集合。
fn inventory(db: &Database, collection: &str) -> Option<IdRepository> {
    Some(match collection {
        "stock_adjustment_lines" => db.stock_adjustment_lines().ids(),
        "stock_adjustments" => db.stock_adjustments().ids(),
        "stock_balances" => db.stock_balances().ids(),
        "stock_movements" => db.stock_movements().ids(),
        "stock_reservation_entries" => db.stock_reservation_entries().ids(),
        "stock_reservations" => db.stock_reservations().ids(),
        _ => return None,
    })
}

/// 绑定 erp_supply 拥有的清理集合。
fn supply(db: &Database, collection: &str) -> Option<IdRepository> {
    Some(match collection {
        "supplier_fulfillment_items" => db.supplier_fulfillment_items().ids(),
        "supplier_fulfillment_orders" => db.supplier_fulfillment_orders().ids(),
        "supplier_offering_availabilities" => db.supplier_offering_availabilities().ids(),
        "supplier_offering_revisions" => db.supplier_offering_revisions().ids(),
        "supplier_offerings" => db.supplier_offerings().ids(),
        "supplier_order_action_lines" => db.supplier_order_action_lines().ids(),
        "supplier_order_actions" => db.supplier_order_actions().ids(),
        "supplier_order_status_histories" => db.supplier_order_status_histories().ids(),
        "supplier_refund_allocations" => db.supplier_refund_allocations().ids(),
        "supplier_refund_facts" => db.supplier_refund_facts().ids(),
        "supplier_settlement_difference_evidence" => db.supplier_settlement_difference_evidence().ids(),
        "supplier_settlement_differences" => db.supplier_settlement_differences().ids(),
        "supplier_settlement_items" => db.supplier_settlement_items().ids(),
        "supplier_settlement_statements" => db.supplier_settlement_statements().ids(),
        _ => return None,
    })
}
