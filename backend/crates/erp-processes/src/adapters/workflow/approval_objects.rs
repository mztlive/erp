//! 审批当前范围事实批量读取；每种强实体及父链按去重 ID 一次查询。

use crate::{Error, Result};
use erp_customer::CustomerExt;
use erp_finance::repository::{PayableExt, ReceivableExt};
use erp_inventory::InventoryExt;
use erp_procurement::repository::PurchaseOrderExt;
use erp_read_models::workbench::authority::WorkItemFactsReader;
use erp_returns::repository::ReturnsExt;
use erp_sales::repository::SalesOrderExt;
use erp_supplier::SupplierExt;
use erp_workflow::entity::document_registry::DocumentType;
use erp_workflow::ports::{OrderTaskSource, WorkflowScopeObject, WorkflowScopeObjects};
use mongodb::Database;
use persistence_core::Executor;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

type Keys = HashSet<(DocumentType, String)>;

pub(super) async fn load(
    db: &Database,
    keys: &Keys,
    executor: &mut dyn Executor,
) -> Result<WorkflowScopeObjects> {
    if keys.len() > 500 {
        return Err(Error::ValidationError("单批审批范围事实超过 500 条".into()));
    }
    let mut result = orders(db, keys, executor).await?;
    let stock_ids = ids(keys, DocumentType::StockAdjustment);
    for row in db
        .stock_adjustments()
        .list_active_by_ids(&stock_ids, executor)
        .await?
    {
        result.insert(
            (DocumentType::StockAdjustment, row.base.id),
            WorkflowScopeObject {
                owner_user_id: row.created_by,
                warehouse_id: Some(row.warehouse_id.to_string()),
                ..Default::default()
            },
        );
    }
    let request_ids = ids(keys, DocumentType::SalesInvoiceRequest);
    for row in db
        .sales_invoice_requests()
        .list_active_by_ids(&request_ids, executor)
        .await?
    {
        result.insert(
            (DocumentType::SalesInvoiceRequest, row.base.id),
            party(row.created_by, row.counterparty_party_id.to_string()),
        );
    }
    let receipt_ids = ids(keys, DocumentType::CustomerReceipt);
    for row in db
        .customer_receipts()
        .list_active_by_ids(&receipt_ids, executor)
        .await?
    {
        result.insert(
            (DocumentType::CustomerReceipt, row.base.id),
            party(row.created_by, row.counterparty_party_id.to_string()),
        );
    }
    refunds(db, keys, &mut result, executor).await?;
    reversals(db, keys, &mut result, executor).await?;
    Ok(result)
}

fn ids(keys: &Keys, kind: DocumentType) -> Vec<String> {
    keys.iter()
        .filter(|(k, _)| *k == kind)
        .map(|(_, id)| id.clone())
        .collect()
}
fn party(owner: String, id: String) -> WorkflowScopeObject {
    WorkflowScopeObject {
        owner_user_id: owner,
        settlement_party_id: Some(id),
        ..Default::default()
    }
}

async fn orders(db: &Database, keys: &Keys, executor: &mut dyn Executor) -> Result<WorkflowScopeObjects> {
    let fact_keys = keys
        .iter()
        .filter_map(|(kind, id)| OrderTaskSource::approval_kind(*kind).map(|k| (k, id.clone())))
        .collect();
    let facts = WorkItemFactsReader::new(db.clone())
        .load(&fact_keys, executor)
        .await?;
    let sources = facts
        .values()
        .filter_map(|f| f.order_scope_source.clone())
        .collect::<BTreeSet<_>>();
    let sales = sources
        .iter()
        .filter_map(|s| {
            if let OrderTaskSource::Sales(id) = s {
                Some(id.clone())
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    let purchases = sources
        .iter()
        .filter_map(|s| {
            if let OrderTaskSource::Purchase(id) = s {
                Some(id.clone())
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    let mut by_source = BTreeMap::new();
    for row in db.sales_orders().list_active_by_ids(&sales, executor).await? {
        let source = OrderTaskSource::Sales(row.base.id);
        by_source.insert(
            source.clone(),
            WorkflowScopeObject {
                customer_id: Some(row.customer_id.to_string()),
                owner_user_id: row.sales_owner_user_id,
                business_org_unit_id: Some(row.business_org_unit_id),
                order_source: Some(source),
                ..Default::default()
            },
        );
    }
    for row in db
        .purchase_orders()
        .list_active_by_ids(&purchases, executor)
        .await?
    {
        let owner = row.current_owner_user_id()?.to_string();
        let source = OrderTaskSource::Purchase(row.base.id);
        by_source.insert(
            source.clone(),
            WorkflowScopeObject {
                owner_user_id: owner,
                business_org_unit_id: Some(row.business_org_unit_id),
                order_source: Some(source),
                ..Default::default()
            },
        );
    }
    let mut result = HashMap::new();
    for (kind, id) in keys {
        let Some(fact_kind) = OrderTaskSource::approval_kind(*kind) else {
            continue;
        };
        let Some(fact) = facts.get(&(fact_kind, id.clone())) else {
            continue;
        };
        let source = fact
            .order_scope_source
            .as_ref()
            .filter(|s| s.matches_kind(fact_kind))
            .ok_or_else(|| Error::Internal("审批订单缺少权威来源".into()))?;
        if let Some(object) = by_source.get(source) {
            result.insert((*kind, id.clone()), object.clone());
        }
    }
    Ok(result)
}

async fn refunds(
    db: &Database,
    keys: &Keys,
    result: &mut WorkflowScopeObjects,
    executor: &mut dyn Executor,
) -> Result<()> {
    let customers = db
        .customer_refunds()
        .list_active_by_ids(&ids(keys, DocumentType::CustomerRefund), executor)
        .await?;
    let customer_ids = customers
        .iter()
        .map(|r| r.customer_id.to_string())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let parties = db
        .customer_accounts()
        .list_active_by_ids(&customer_ids, executor)
        .await?
        .into_iter()
        .map(|r| (r.base.id, r.party_id.to_string()))
        .collect::<HashMap<_, _>>();
    for row in customers {
        if let Some(id) = parties.get(row.customer_id.as_ref()) {
            result.insert(
                (DocumentType::CustomerRefund, row.base.id),
                party(row.created_by, id.clone()),
            );
        }
    }
    let suppliers = db
        .supplier_refunds()
        .list_active_by_ids(&ids(keys, DocumentType::SupplierRefund), executor)
        .await?;
    let supplier_ids = suppliers
        .iter()
        .map(|r| r.supplier_id.to_string())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let parties = db
        .supplier_accounts()
        .list_active_by_ids(&supplier_ids, executor)
        .await?
        .into_iter()
        .map(|r| (r.base.id, r.party_id.to_string()))
        .collect::<HashMap<_, _>>();
    for row in suppliers {
        if let Some(id) = parties.get(row.supplier_id.as_ref()) {
            result.insert(
                (DocumentType::SupplierRefund, row.base.id),
                party(row.created_by, id.clone()),
            );
        }
    }
    Ok(())
}

async fn reversals(
    db: &Database,
    keys: &Keys,
    result: &mut WorkflowScopeObjects,
    executor: &mut dyn Executor,
) -> Result<()> {
    let rows = db
        .receipt_reversals()
        .list_active_by_ids(&ids(keys, DocumentType::ReceiptReversal), executor)
        .await?;
    let receipt_ids = rows
        .iter()
        .map(|r| r.original_customer_receipt_id.to_string())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let parties = db
        .customer_receipts()
        .list_active_by_ids(&receipt_ids, executor)
        .await?
        .into_iter()
        .map(|r| (r.base.id, r.counterparty_party_id.to_string()))
        .collect::<HashMap<_, _>>();
    for row in rows {
        if let Some(id) = parties.get(row.original_customer_receipt_id.as_ref()) {
            result.insert(
                (DocumentType::ReceiptReversal, row.base.id),
                party(row.created_by, id.clone()),
            );
        }
    }
    let rows = db
        .payment_reversals()
        .list_active_by_ids(&ids(keys, DocumentType::PaymentReversal), executor)
        .await?;
    let payment_ids = rows
        .iter()
        .map(|r| r.original_supplier_payment_id.to_string())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let payments = db
        .supplier_payments()
        .list_active_by_ids(&payment_ids, executor)
        .await?;
    let supplier_ids = payments
        .iter()
        .map(|p| p.supplier_id.to_string())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let suppliers = db
        .supplier_accounts()
        .list_active_by_ids(&supplier_ids, executor)
        .await?
        .into_iter()
        .map(|s| (s.base.id, s.party_id.to_string()))
        .collect::<HashMap<_, _>>();
    let parties = payments
        .into_iter()
        .filter_map(|p| {
            suppliers
                .get(p.supplier_id.as_ref())
                .map(|party| (p.base.id, party.clone()))
        })
        .collect::<HashMap<_, _>>();
    for row in rows {
        if let Some(id) = parties.get(row.original_supplier_payment_id.as_ref()) {
            result.insert(
                (DocumentType::PaymentReversal, row.base.id),
                party(row.created_by, id.clone()),
            );
        }
    }
    Ok(())
}
