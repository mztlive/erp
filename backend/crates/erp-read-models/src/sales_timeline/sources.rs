use std::collections::BTreeMap;

use erp_workflow::DocumentType;

use super::{TimelineCategory, TimelineEvent};
use crate::{Error, Result};

/// 时间线精确单据身份及来源；不承载单据敏感内容。
#[derive(Debug, Clone)]
pub struct TimelineDocument {
    pub kind: DocumentType,
    pub id: String,
    pub number: String,
    pub sales_order_id: Option<String>,
    pub purchase_order_id: Option<String>,
    pub(super) category_override: Option<TimelineCategory>,
}

pub(super) struct Sources {
    pub documents: BTreeMap<String, TimelineDocument>,
    pub facts: Vec<TimelineEvent>,
}

impl Sources {
    pub(super) fn new() -> Self {
        Self { documents: BTreeMap::new(), facts: Vec::new() }
    }

    pub(super) fn add(&mut self, kind: DocumentType, id: String, sale: Option<&str>, purchase: Option<&str>) {
        self.documents.entry(id.clone()).or_insert_with(|| {
            let mut document = TimelineDocument::new(kind, id);
            document.sales_order_id = sale.map(str::to_owned);
            document.purchase_order_id = purchase.map(str::to_owned);
            document
        });
    }

    pub(super) fn ids(&self, kind: DocumentType) -> Vec<String> {
        self.documents.values().filter(|doc| doc.kind == kind).map(|doc| doc.id.clone()).collect()
    }

    pub(super) fn check_size(&self) -> Result<()> {
        if self.documents.len() > 2000 || self.facts.len() > 10000 {
            return Err(Error::ValidationError("关联记录过多，请按单据查看办理动态".into()));
        }
        Ok(())
    }
}

impl TimelineDocument {
    pub(super) fn new(kind: DocumentType, id: String) -> Self {
        Self {
            kind,
            id,
            number: String::new(),
            sales_order_id: None,
            purchase_order_id: None,
            category_override: None,
        }
    }

    /// 仅沿当前单据的采购外键补齐销售来源，不覆盖已有直接销售外键。
    pub(super) fn resolve_purchase_source(&mut self, purchase_id: &str, sales_id: &str) {
        if self.sales_order_id.is_none() && self.purchase_order_id.as_deref() == Some(purchase_id) {
            self.sales_order_id = Some(sales_id.into());
        }
    }

    pub(super) fn category(&self) -> TimelineCategory {
        use DocumentType::*;
        if let Some(category) = self.category_override {
            return category;
        }
        match self.kind {
            SalesOrder | VoucherSalesOrder | SalesChangeOrder => TimelineCategory::Sales,
            PurchaseOrder | PurchaseChangeOrder => TimelineCategory::Procurement,
            PurchaseReceipt | Delivery | ElectronicDelivery | ServiceFulfillment | CustomerAcceptance => {
                TimelineCategory::Fulfillment
            },
            CustomerReceipt | SalesInvoiceRequest | Invoice => TimelineCategory::Receivable,
            SupplierPayment | PurchasePaymentRequest => TimelineCategory::Payable,
            _ => TimelineCategory::Returns,
        }
    }

    pub(super) fn event(&self, id: String, time: u64, title: String, actor: String) -> TimelineEvent {
        TimelineEvent {
            id,
            occurred_at: time,
            category: self.category(),
            title,
            actor_name: actor,
            actor_id: None,
            document_type: self.kind.as_str().into(),
            document_id: self.id.clone(),
            document_number: self.number.clone(),
            document_label: self.kind.label().into(),
            href: None,
            details: Vec::new(),
            amount: None,
            amount_label: None,
            business_date: None,
            after_submission: false,
        }
    }
}
