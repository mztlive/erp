//! 业务单据关联的财务文件；仅返回文件展示字段，存储位置不进入业务响应。

use std::collections::{BTreeMap, BTreeSet};

use application_core::AuditActor;
use erp_core::ids::{
    BusinessDocumentId, PayableAccountId, PurchaseOrderId, ReceivableAccountId, SupplierPaymentId,
};
use erp_core::money::Amount;
use erp_finance::entity::payable::{PaymentAllocation, SupplierPaymentStatus};
use erp_finance::entity::receivable::{InvoiceDirection, InvoiceStatus};
use erp_finance::repository::prelude::*;
use erp_finance::repository::{PayableExt, ReceivableExt};
use erp_fulfillment::repository::FulfillmentExt;
use erp_identity::SharedRbacService;
use erp_support::repository::prelude::*;
use erp_support::{FileAsset, FileAssetExt, SecurityScanStatus};
use mongodb::Database;
use persistence_core::{Executor, Transactional};
use serde::Serialize;

use crate::sales_center::access::SalesAccess;
use crate::{Error, Result};

/// 业务文件列表中的安全展示字段。
#[derive(Debug, Clone, Serialize)]
pub struct FinancialFileView {
    /// 所属发票或付款单身份。
    pub document_id: String,
    /// 财务业务单号。
    pub document_no: String,
    /// 用于精准匹配文件的内部引用，不提供通用资产访问资格。
    pub file_asset_id: String,
    /// 用户文件名。
    pub file_name: String,
    /// 内容类型。
    pub content_type: String,
    /// 文件大小。
    pub byte_size: u64,
}

impl FinancialFileView {
    /// 只从财务来源关系及文件展示字段构造响应。
    fn new(document_id: String, document_no: String, file: FileAsset) -> Self {
        Self {
            document_id,
            document_no,
            file_asset_id: file.base.id,
            file_name: file.file_name,
            content_type: file.content_type,
            byte_size: file.byte_size,
        }
    }
}

/// 从已授权业务来源读取财务文件的组合服务。
#[derive(Clone)]
pub struct BusinessFinanceFiles {
    db: Database,
    rbac: SharedRbacService,
}

impl BusinessFinanceFiles {
    /// 装配数据库及当前销售范围授权源。
    /// # 参数
    /// `db` 为各域事实源，`rbac` 为权限服务。
    /// # 返回
    /// 返回无授权缓存的读取服务。
    /// # 错误
    /// 无。
    pub fn new(db: Database, rbac: SharedRbacService) -> Self {
        Self { db, rbac }
    }

    /// 在同一事务证明销售单详情资格并读取该单的已登记发票附件。
    /// # 参数
    /// `actor` 为当前账号，`sales_id` 为来源销售单。
    /// # 返回
    /// 返回本单发票文件，不包含整张发票金额或其他单据分配。
    /// # 错误
    /// 来源不可见或关联事实读取失败时拒绝。
    pub async fn sales(&self, actor: &AuditActor, sales_id: &str) -> Result<Vec<FinancialFileView>> {
        let this = self.clone();
        let actor = actor.clone();
        let sales_id = sales_id.to_string();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    SalesAccess::new(this.db.clone(), this.rbac.clone())
                        .require_object(&actor, "detail", &sales_id, &[], executor)
                        .await?;
                    this.sales_files(&sales_id, executor).await
                })
            })
            .await
    }

    /// 沿应收子账与有效发票分配读取文件，禁止按客户或往来主体扩大范围。
    async fn sales_files(
        &self,
        sales_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Vec<FinancialFileView>> {
        let accounts =
            self.db.receivable_accounts().find_accounts_by_sales_order_id(sales_id, executor).await?;
        let ids = accounts.iter().map(|a| ReceivableAccountId::new(&a.base.id)).collect::<Vec<_>>();
        let allocations =
            self.db.sales_invoice_allocations().find_allocations_by_accounts(&ids, executor).await?;
        let invoice_ids = allocations
            .into_iter()
            .map(|a| a.invoice_id.to_string())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        let invoices = self.db.invoices().find_invoices_by_ids(&invoice_ids, executor).await?;
        let mut result = Vec::new();
        let mut seen_assets = BTreeSet::new();
        for invoice in invoices.into_iter().filter(|i| {
            i.invoice_direction == InvoiceDirection::Sales && published_invoice(i.stable.status())
        }) {
            let attachments = self
                .db
                .document_attachments()
                .list_by_document(&BusinessDocumentId::new(&invoice.base.id), executor)
                .await?;
            let ids = attachments.into_iter().map(|a| a.file_asset_id).collect::<Vec<_>>();
            for file in self.db.file_assets().find_by_ids(&ids, executor).await? {
                if readable_file(&file) && seen_assets.insert(file.base.id.clone()) {
                    result.push(FinancialFileView::new(
                        invoice.base.id.clone(),
                        invoice.invoice_no.clone(),
                        file,
                    ));
                }
            }
        }
        Ok(result)
    }

    /// 从正式授权的履约任务对象推导采购单，再读取该单已付款的银行回单。
    /// # 参数
    /// `object_type` / `object_id` 必须来自当前工作流授权结果，不接受用户自行提供的来源单。
    /// # 返回
    /// 返回该采购来源关联的银行回单。
    /// # 错误
    /// 非采购履约对象、缺失来源或读取失败时拒绝。
    pub async fn fulfillment(&self, object_type: &str, object_id: &str) -> Result<Vec<FinancialFileView>> {
        let this = self.clone();
        let kind = object_type.to_string();
        let id = object_id.to_string();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let purchase_id = this.purchase_source(&kind, &id, executor).await?;
                    this.payment_files(&purchase_id, executor).await
                })
            })
            .await
    }

    /// 强类型履约来源只允许采购入库、供应商直发、电子交付与服务履约。
    async fn purchase_source(
        &self,
        kind: &str,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<PurchaseOrderId> {
        let missing = || Error::NotFound("采购履约来源不存在".into());
        match kind {
            "purchase_receipt" => Ok(self
                .db
                .purchase_receipts()
                .find_by_id(id, executor)
                .await?
                .ok_or_else(missing)?
                .purchase_order_id),
            "delivery" => self
                .db
                .deliveries()
                .find_by_id(id, executor)
                .await?
                .ok_or_else(missing)?
                .purchase_order_id
                .ok_or_else(missing),
            "electronic_delivery" => Ok(self
                .db
                .electronic_deliveries()
                .find_by_id(id, executor)
                .await?
                .ok_or_else(missing)?
                .purchase_order_id),
            "service_fulfillment" => Ok(self
                .db
                .service_fulfillments()
                .find_by_id(id, executor)
                .await?
                .ok_or_else(missing)?
                .purchase_order_id),
            _ => Err(missing()),
        }
    }

    /// 精确沿采购应付分录、核销分配和付款单读取回单，去重后支持多笔付款。
    async fn payment_files(
        &self,
        purchase_id: &PurchaseOrderId,
        executor: &mut dyn Executor,
    ) -> Result<Vec<FinancialFileView>> {
        let Some(account) = self.db.payable_accounts().find_by_purchase_order(purchase_id, executor).await?
        else {
            return Ok(vec![]);
        };
        let entries = self
            .db
            .payable_entries()
            .find_entries_by_accounts(&[PayableAccountId::new(account.base.id)], executor)
            .await?;
        let ids = entries.into_iter().map(|e| e.base.id.into()).collect::<Vec<_>>();
        let allocations = self.db.payment_allocations().find_allocations_by_entries(&ids, executor).await?;
        let payment_ids = effective_payment_ids(&allocations);
        let payments =
            self.db.supplier_payments().find_supplier_payments_by_ids(&payment_ids, executor).await?;
        let mut result = vec![];
        let mut seen_assets = BTreeSet::new();
        for payment in payments.into_iter().filter(|p| p.status == SupplierPaymentStatus::Posted) {
            if let Some(id) = payment.bank_receipt_asset_id {
                let file = self
                    .db
                    .file_assets()
                    .find_by_id(&id, executor)
                    .await?
                    .ok_or_else(|| Error::NotFound("付款回单不存在".into()))?;
                if readable_file(&file) && seen_assets.insert(file.base.id.clone()) {
                    result.push(FinancialFileView::new(payment.base.id, payment.payment_no, file));
                }
            }
        }
        Ok(result)
    }
}

/// 正式发票包含已登记票及已由红票冲销的原票；草稿没有下载资格。
fn published_invoice(status: InvoiceStatus) -> bool {
    matches!(status, InvoiceStatus::Registered | InvoiceStatus::RedInvoiced)
}

/// 列表与下载同时关闭被销毁、隔离或无有效内容的文件。
fn readable_file(file: &FileAsset) -> bool {
    file.destroyed_at.is_none()
        && file.byte_size > 0
        && !matches!(
            file.security_scan_status,
            SecurityScanStatus::Rejected | SecurityScanStatus::Quarantined
        )
        && matches!(file.content_type.as_str(), "application/pdf" | "image/jpeg" | "image/png" | "image/webp")
}

/// 只返回在本采购应付上仍有正向净核销额的付款，全部冲减不能保留下载资格。
fn effective_payment_ids(allocations: &[PaymentAllocation]) -> Vec<SupplierPaymentId> {
    let mut totals = BTreeMap::new();
    for row in allocations {
        let total = totals.entry(row.supplier_payment_id.to_string()).or_insert_with(Amount::zero);
        *total = row.allocation_action.apply_to_net(*total, row.allocated_amount);
    }
    totals
        .into_iter()
        .filter(|(_, amount)| *amount > Amount::zero())
        .map(|(id, _)| SupplierPaymentId::new(id))
        .collect()
}

/// 对下载请求按所属财务单和文件双键精确匹配，禁止跨单或替换资产。
///
/// # 参数
/// * `files` - 当前财务单可见文件。
/// * `document_id` - 所属财务单身份。
/// * `asset_id` - 指定文件时必须同时命中；为空则只按财务单匹配。
///
/// # 返回
/// 返回第一笔同时命中的文件视图。
///
/// # 错误
/// 没有同时命中财务单和文件时返回 `NotFound`。
pub fn matching_file<'a>(
    files: &'a [FinancialFileView],
    document_id: &str,
    asset_id: Option<&str>,
) -> Result<&'a FinancialFileView> {
    files
        .iter()
        .find(|file| file.document_id == document_id && asset_id.is_none_or(|id| file.file_asset_id == id))
        .ok_or_else(|| Error::NotFound("财务文件不存在或无权下载".into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn download_match_never_accepts_another_document_or_asset() {
        let files = vec![FinancialFileView {
            document_id: "invoice-a".into(),
            document_no: "INV-1".into(),
            file_asset_id: "asset-a".into(),
            file_name: "发票.pdf".into(),
            content_type: "application/pdf".into(),
            byte_size: 12,
        }];
        assert!(matching_file(&files, "invoice-a", Some("asset-a")).is_ok());
        assert!(matching_file(&files, "invoice-b", Some("asset-a")).is_err());
        assert!(matching_file(&files, "invoice-a", Some("asset-b")).is_err());
        assert!(matching_file(&[], "invoice-a", None).is_err());
    }

    #[test]
    fn only_positive_net_payment_allocations_keep_the_source_relation() {
        use erp_core::common::time::Instant;
        use erp_core::ids::{PayableEntryId, PaymentAllocationId};
        use erp_finance::entity::payable::{AllocationAction, PaymentAllocationData};
        let allocation = |payment: &str, action, amount: &str| {
            PaymentAllocation::new(
                PaymentAllocationId::new("a"),
                PaymentAllocationData {
                    supplier_payment_id: SupplierPaymentId::new(payment),
                    payable_entry_id: PayableEntryId::new("entry"),
                    allocation_seq: 1,
                    allocation_action: action,
                    allocated_amount: amount.parse().unwrap(),
                    allocated_at: Instant::now(),
                    reverses_allocation_id: (action == AllocationAction::Reverse)
                        .then(|| PaymentAllocationId::new("original")),
                },
            )
            .unwrap()
        };
        let rows = [
            allocation("positive", AllocationAction::Apply, "10"),
            allocation("positive", AllocationAction::Reverse, "3"),
            allocation("reversed", AllocationAction::Apply, "5"),
            allocation("reversed", AllocationAction::Reverse, "5"),
        ];
        assert_eq!(effective_payment_ids(&rows), vec![SupplierPaymentId::new("positive")]);
        assert!(effective_payment_ids(&[]).is_empty());
    }

    #[test]
    fn governance_closes_destroyed_rejected_quarantined_or_empty_files() {
        use erp_core::common::time::Instant;
        use erp_core::ids::FileAssetId;
        use erp_support::{RegisterFileAssetRequest, RetentionClass, SensitivityClass};
        let request = RegisterFileAssetRequest {
            storage_object_key: "object".into(),
            file_name: "发票.pdf".into(),
            content_type: "application/pdf".into(),
            byte_size: 12,
            content_hmac: "a".repeat(64),
            sensitivity_class: SensitivityClass::Sensitive,
            retention_class: RetentionClass::LongTerm,
            expires_at: None,
        };
        let mut file = FileAsset::new(FileAssetId::new("file"), request.into_data("actor").unwrap()).unwrap();
        assert!(readable_file(&file));
        for status in [SecurityScanStatus::Rejected, SecurityScanStatus::Quarantined] {
            file.security_scan_status = status;
            assert!(!readable_file(&file));
        }
        file.security_scan_status = SecurityScanStatus::Pending;
        file.destroyed_at = Some(Instant::now());
        assert!(!readable_file(&file));
        file.destroyed_at = None;
        file.byte_size = 0;
        assert!(!readable_file(&file));
        assert!(!published_invoice(InvoiceStatus::Draft));
        assert!(published_invoice(InvoiceStatus::Registered));
        assert!(published_invoice(InvoiceStatus::RedInvoiced));
    }
}
