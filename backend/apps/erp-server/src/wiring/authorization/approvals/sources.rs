//! 将冻结审批对象沿权威业务外键解析到已有根事实读取器。
use std::collections::{BTreeMap, HashSet};
use std::sync::Arc;

use async_trait::async_trait;
use erp_core::ids::{CustomerRefundId, PaymentReversalId, ReceiptReversalId, SupplierRefundId};
use erp_finance::repository::{PayableExt, ReceivableExt};
use erp_identity::entity::authorization::{AccessContext, RelatedObjects};
use erp_identity::{AccessFactsPort, Error, ResourceTargets, Result};
use erp_read_models::Error as ReadError;
use erp_read_models::workbench::authority::WorkItemFactsReader;
use erp_returns::repository::ReturnsExt;
use erp_workflow::entity::document_registry::DocumentType;
use erp_workflow::ports::{ApprovalSubjectFactsPort, OrderTaskSource};
use mongodb::Database;
use persistence_core::Executor;

use super::super::FactsBindings;

pub(super) struct SubjectFacts {
    db: Database,
    roots: BTreeMap<String, Arc<dyn AccessFactsPort>>,
}
impl SubjectFacts {
    /// 复用已构造的根事实读取器，不创建请求状态或授权服务。
    /// # 参数
    /// db 为共享数据库，roots 为各拥有领域按资源提供的唯一事实实例。
    /// # 返回
    /// 按真实业务强外键分发的审批源适配器。
    /// # 错误
    /// 不执行 I/O；缺少实际所需根读取器在读取时返回接入错误。
    pub(super) fn new(db: Database, roots: &FactsBindings) -> Self {
        Self { db, roots: roots.iter().cloned().collect() }
    }

    async fn source(
        &self,
        kind: DocumentType,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<(&'static str, String)> {
        if OrderTaskSource::approval_kind(kind).is_some() {
            return self.order_source(kind, id, executor).await;
        }
        let source = match kind {
            DocumentType::StockAdjustment => Some(("stock_adjustment", id.into())),
            DocumentType::CustomerReceipt => Some(("customer_receipt", id.into())),
            DocumentType::SalesInvoiceRequest => Some(("sales_invoice_request", id.into())),
            DocumentType::PurchasePaymentRequest => Some(("purchase_payment_request", id.into())),
            DocumentType::SupplierPayment => Some(("supplier_payment", id.into())),
            DocumentType::CustomerRefund => self.customer_refund_source(id, executor).await?,
            DocumentType::SupplierRefund => self.supplier_refund_source(id, executor).await?,
            DocumentType::ReceiptReversal => self
                .db
                .receipt_reversals()
                .find_by_id(&ReceiptReversalId::new(id), executor)
                .await?
                .map(|row| ("customer_receipt", row.original_customer_receipt_id.to_string())),
            DocumentType::PaymentReversal => self
                .db
                .payment_reversals()
                .find_by_id(&PaymentReversalId::new(id), executor)
                .await?
                .map(|row| ("supplier_payment", row.original_supplier_payment_id.to_string())),
            _ => return Err(Error::ValidationError("审批类型未接入真实来源".into())),
        };
        source.ok_or_else(|| Error::NotFound("审批真实业务来源不存在或不完整".into()))
    }

    async fn order_source(
        &self,
        kind: DocumentType,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<(&'static str, String)> {
        let kind = OrderTaskSource::approval_kind(kind)
            .ok_or_else(|| Error::ValidationError("此审批不是订单来源类型".into()))?;
        let key = (kind, id.to_string());
        let facts = WorkItemFactsReader::new(self.db.clone())
            .load(&HashSet::from([key.clone()]), executor)
            .await
            .map_err(read_error)?;
        match facts
            .get(&key)
            .and_then(|fact| fact.order_scope_source.as_ref())
            .filter(|source| source.matches_kind(kind))
        {
            Some(OrderTaskSource::Sales(id)) => Ok(("sales_order", id.clone())),
            Some(OrderTaskSource::Purchase(id)) => Ok(("purchase_order", id.clone())),
            None => Err(Error::NotFound("审批订单缺少权威业务来源".into())),
        }
    }

    async fn customer_refund_source(
        &self,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<(&'static str, String)>> {
        let Some(row) = self.db.customer_refunds().find_by_id(&CustomerRefundId::new(id), executor).await?
        else {
            return Ok(None);
        };
        match (row.original_receipt_id, row.original_receivable_entry_id) {
            (Some(id), None) => Ok(Some(("customer_receipt", id.to_string()))),
            (None, Some(id)) => Ok(self
                .db
                .receivable_entries()
                .find_by_id(&id, executor)
                .await?
                .map(|entry| ("receivable_account", entry.receivable_account_id.to_string()))),
            _ => Ok(None),
        }
    }

    async fn supplier_refund_source(
        &self,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<(&'static str, String)>> {
        let Some(row) = self.db.supplier_refunds().find_by_id(&SupplierRefundId::new(id), executor).await?
        else {
            return Ok(None);
        };
        match (row.original_payment_id, row.original_payable_entry_id) {
            (Some(id), None) => Ok(Some(("supplier_payment", id.to_string()))),
            (None, Some(id)) => Ok(self
                .db
                .payable_entries()
                .find_by_id(&id, executor)
                .await?
                .map(|entry| ("payable_account", entry.payable_account_id.to_string()))),
            _ => Ok(None),
        }
    }
}

#[async_trait]
impl ApprovalSubjectFactsPort for SubjectFacts {
    async fn load(
        &self,
        context: &AccessContext,
        document_type: &str,
        business_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<RelatedObjects> {
        let kind = DocumentType::try_from_code(document_type)
            .map_err(|error| Error::ValidationError(error.to_string()))?;
        let (resource, id) = self.source(kind, business_id, executor).await?;
        let reader = self
            .roots
            .get(resource)
            .ok_or_else(|| Error::ValidationError(format!("审批来源缺少事实绑定：{resource}")))?;
        let mut source_context = context.clone();
        source_context.resource = resource.into();
        let ids = ResourceTargets::new(&[id.as_str()])?;
        let objects = reader.load(&source_context, &ids, executor).await?;
        Ok(RelatedObjects { expected_ids: [id].into(), objects })
    }
}

fn read_error(error: ReadError) -> Error {
    match error {
        ReadError::Internal(value) => Error::Internal(value),
        ReadError::NotFound(value) => Error::NotFound(value),
        ReadError::ValidationError(value) => Error::ValidationError(value),
        ReadError::InvalidInput(value) => Error::InvalidInput(value),
        ReadError::BusinessLogicError(value) => Error::BusinessLogicError(value),
        ReadError::ConflictError(value) => Error::ConflictError(value),
        ReadError::ReceiptDuplicate(value) => Error::ReceiptDuplicate(value),
        ReadError::TransientTransaction(value) => Error::TransientTransaction(value),
        ReadError::Forbidden(value) => Error::Forbidden(value),
        ReadError::Unauthenticated(value) => Error::Unauthenticated(value),
        ReadError::Logic(value) => Error::Logic(value),
        ReadError::Rbac(value) => Error::Rbac(value),
        ReadError::OutcomeUnknown(value) => Error::OutcomeUnknown(value),
        ReadError::RepositoryError(value) => Error::RepositoryError(value),
        ReadError::Coded(value) => Error::BusinessLogicError(value.to_string()),
    }
}
