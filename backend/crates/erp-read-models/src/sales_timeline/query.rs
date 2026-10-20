use application_core::AuditActor;
use erp_workflow::DocumentType;
use persistence_core::{Executor, Transactional};

use super::repository::sources::discover;
use super::sources::Sources;
use super::{SalesTimelineReadService, TimelinePage, TimelineQuery, dto, history};
use crate::Result;
use crate::sales_center::access::SalesAccess;

impl SalesTimelineReadService {
    /// 在同一读取事务校验销售单和关联单据权限后返回时间线。
    ///
    /// # 参数
    /// `actor` 为认证账号，`id` 为销售单，`query` 为业务分类与游标。
    /// # 返回
    /// 返回有权查看的历史事件及下一页位置；不修改业务状态。
    /// # 错误
    /// 权限不足、参数无效及数据库故障均返回错误。
    pub async fn sales(&self, actor: &AuditActor, id: &str, query: TimelineQuery) -> Result<TimelinePage> {
        query.limit()?;
        query.cursor()?;
        let this = self.clone();
        let actor = actor.clone();
        let id = id.to_owned();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let order = SalesAccess::new(this.db.clone(), this.rbac.clone())
                        .require_object(&actor, "detail", &id, &[], executor)
                        .await?;
                    let mut sources = discover(&this.db, &id, executor).await?;
                    if let Some(document) = sources.documents.get_mut(&id) {
                        document.number = order.order_no;
                    }
                    authorize(this.access.as_ref(), &actor, &mut sources, executor).await?;
                    this.read(sources, &query, None, executor).await
                })
            })
            .await
    }

    pub(super) async fn read(
        &self,
        sources: Sources,
        query: &TimelineQuery,
        submitted_at: Option<u64>,
        executor: &mut dyn Executor,
    ) -> Result<TimelinePage> {
        let mut events = history::history(&self.db, &sources.documents, query, executor).await?;
        events.extend(sources.facts);
        for event in &mut events {
            if let Some(document) = sources.documents.get(&event.document_id) {
                if event.document_number.is_empty() {
                    event.document_number.clone_from(&document.number);
                }
                event.href = href(document);
            }
            event.after_submission = submitted_at.is_some_and(|at| event.occurred_at > at);
        }
        let mut page = dto::page(events, query)?;
        super::repository::names::names(&self.db, &mut page.items, executor).await?;
        Ok(page)
    }
}

pub(super) fn href(document: &super::TimelineDocument) -> Option<String> {
    use DocumentType::*;
    let id = &document.id;
    let path = match document.kind {
        SalesOrder | VoucherSalesOrder => format!("/sales/orders/{id}"),
        SalesChangeOrder => format!("/sales/orders/{}?section=versions", document.sales_order_id.as_deref()?),
        PurchaseOrder => format!("/procurement/orders/{id}"),
        PurchasePaymentRequest => format!("/procurement/payment-requests/{id}"),
        SalesInvoiceRequest => format!("/finance/customer-accounts/invoice-requests?requestId={id}"),
        CustomerReceipt => customer_href(id, "receipt", "receipt"),
        CustomerRefund => customer_href(id, "refund", "receipt"),
        ReceiptReversal => customer_href(id, "reversal", "receipt"),
        SupplierPayment => supplier_href(id, "payment", "payment"),
        SupplierRefund => supplier_href(id, "refund", "payment"),
        PaymentReversal => supplier_href(id, "reversal", "payment"),
        Invoice if document.category() == super::TimelineCategory::Payable => {
            supplier_href(id, "invoice", "purchase_invoice")
        },
        Invoice => customer_href(id, "invoice", "sales_invoice"),
        _ => return None,
    };
    Some(path)
}

fn customer_href(id: &str, kind: &str, view: &str) -> String {
    format!("/finance/customer-accounts?view={view}&previewKind={kind}&previewId={id}")
}

fn supplier_href(id: &str, kind: &str, view: &str) -> String {
    format!("/finance/supplier-accounts?view={view}&previewKind={kind}&detailId={id}")
}

pub(super) async fn authorize(
    access: &dyn super::TimelineAccessPort,
    actor: &AuditActor,
    sources: &mut Sources,
    executor: &mut dyn Executor,
) -> Result<()> {
    let mut visible = Vec::new();
    for document in sources.documents.values() {
        if access.readable(actor, document, executor).await? {
            visible.push(document.id.clone());
        }
    }
    sources.documents.retain(|id, _| visible.contains(id));
    sources.facts.retain(|event| sources.documents.contains_key(&event.document_id));
    Ok(())
}
