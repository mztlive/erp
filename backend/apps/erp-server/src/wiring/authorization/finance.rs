//! 全部资金真实根、来源映射及固定读取入口绑定。
use std::collections::BTreeMap;
use std::sync::Arc;

use erp_identity::{AccessFactsPort, AccessMapping, Result};
use erp_permissions::{cost, payable, purchase_payment_request, receivable};
use erp_read_models::finance::funds_scope::{FinancialFacts, FundsMapping};
use mongodb::Database;

use super::{FactsBindings, MappingBindings};

type FinancialFactsMap = BTreeMap<String, Arc<dyn AccessFactsPort>>;

const RESOURCES: &[&str] = &[
    "receivable_account",
    "sales_invoice_request",
    "payable_account",
    "purchase_payment_request",
    "customer_receipt",
    "supplier_payment",
    "invoice",
    "cost_entry",
    "cost_allocation",
    "purchase_invoice_allocation",
    "customer_refund",
    "supplier_refund",
    "receipt_reversal",
    "payment_reversal",
];

// 绑定真实资金根并复用已有销售、采购和结算来源实例。
// db 为共享数据库；三个来源 Mapping 由对应拥有领域提供。
// 唯一根事实、固定查询绑定，以及供任务适配器复用的同一事实实例。
// 固定资源、来源模型或查询描述不匹配时拒绝。
pub(super) fn bindings(
    db: Database,
    sales: Arc<dyn AccessMapping>,
    purchase: Arc<dyn AccessMapping>,
    settlement: Arc<dyn AccessMapping>,
) -> Result<(FactsBindings, MappingBindings, FinancialFactsMap)> {
    let shared = financial_facts(db.clone());
    let facts = shared.iter().map(|(resource, facts)| (resource.clone(), facts.clone())).collect();
    let mut mappings = Vec::new();
    for permissions in [
        vec![receivable::ACCOUNT_LIST, receivable::ACCOUNT_DETAIL],
        vec![
            receivable::INVOICE_REQUEST_INVOICE_REQUEST_LIST,
            receivable::INVOICE_REQUEST_INVOICE_REQUEST_DETAIL,
        ],
        vec![payable::ACCOUNT_LIST, payable::ACCOUNT_DETAIL],
        vec![
            purchase_payment_request::LIST,
            purchase_payment_request::DETAIL,
            purchase_payment_request::CREDIT_LIST,
        ],
        vec![receivable::CUSTOMER_RECEIPT_LIST, receivable::CUSTOMER_RECEIPT_DETAIL],
        vec![payable::SUPPLIER_PAYMENT_LIST, payable::SUPPLIER_PAYMENT_DETAIL],
        vec![receivable::INVOICE_LIST, receivable::INVOICE_DETAIL],
        vec![cost::ENTRY_LIST, cost::ENTRY_DETAIL],
        vec![cost::ALLOCATION_LIST],
        vec![payable::PURCHASE_INVOICE_ALLOCATION_LIST],
    ] {
        let mapping: Arc<dyn AccessMapping> =
            Arc::new(FundsMapping::new(db.clone(), permissions[0].resource())?.with_sources(
                sales.clone(),
                purchase.clone(),
                settlement.clone(),
            )?);
        mappings.extend(permissions.into_iter().map(|permission| (permission, mapping.clone())));
    }
    Ok((facts, mappings, shared))
}

fn financial_facts(db: Database) -> FinancialFactsMap {
    let shared: FinancialFactsMap = RESOURCES
        .iter()
        .copied()
        .map(|resource| {
            let facts: Arc<dyn AccessFactsPort> = Arc::new(FinancialFacts::new(db.clone(), resource));
            (resource.into(), facts)
        })
        .collect();
    shared
}
