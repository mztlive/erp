//! 仅付款申请自身形成的付款事实及回单取得专用资料关系。

use std::collections::BTreeSet;

use erp_finance::entity::payable::{PurchasePaymentRequest, SupplierPayment};
use erp_finance::repository::PayableExt;
use erp_finance::repository::prelude::*;
use erp_support::repository::FileAssetExt;
use erp_workflow::entity::approval_integration::ApprovalMaterialFile;
use persistence_core::Executor;

use super::{PaymentRequestAttachmentView, PaymentRequestPaymentView, PurchasePaymentRequestReadService};
use crate::sales_center::materials::material_from_asset;
use crate::{Error, Result};

impl PurchasePaymentRequestReadService {
    pub(super) async fn payments(
        &self,
        request: &PurchasePaymentRequest,
        executor: &mut dyn Executor,
    ) -> Result<Vec<PaymentRequestPaymentView>> {
        let mut views = Vec::new();
        for payment in self.request_payments(request, executor).await? {
            let receipt =
                self.receipt_file(&payment, executor).await?.map(|file| PaymentRequestAttachmentView {
                    file_asset_id: file.file_asset_id.to_string(),
                    file_name: file.file_name,
                    content_type: file.content_type,
                    byte_size: file.byte_size,
                });
            views.push(PaymentRequestPaymentView {
                effective_amount: payment.effective_amount(),
                payment_id: payment.base.id,
                payment_no: payment.payment_no,
                paid_at: payment.paid_at,
                amount: payment.amount,
                reversed_amount: payment.reversed_amount,
                allocations: payment.request_allocations,
                receipt,
            });
        }
        Ok(views)
    }

    pub(super) async fn payment_receipts(
        &self,
        request: &PurchasePaymentRequest,
        executor: &mut dyn Executor,
    ) -> Result<Vec<ApprovalMaterialFile>> {
        let mut files = Vec::new();
        for payment in self.request_payments(request, executor).await? {
            if let Some(file) = self.receipt_file(&payment, executor).await? {
                files.push(file);
            }
        }
        Ok(files)
    }

    async fn request_payments(
        &self,
        request: &PurchasePaymentRequest,
        executor: &mut dyn Executor,
    ) -> Result<Vec<SupplierPayment>> {
        let rows = self.db.supplier_payments().list_for_payment_request(&request.base.id, executor).await?;
        if rows.len() > 10_000 {
            return Err(Error::ValidationError("付款申请实际付款记录超过读取上限".into()));
        }
        let accounts = request.lines.iter().map(|line| line.payable_account_id.as_str()).collect();
        if rows.iter().any(|payment| !payment_matches(payment, request, &accounts)) {
            return Err(Error::ConflictError("实际付款记录与付款申请批准来源不一致".into()));
        }
        Ok(rows)
    }

    async fn receipt_file(
        &self,
        payment: &SupplierPayment,
        executor: &mut dyn Executor,
    ) -> Result<Option<ApprovalMaterialFile>> {
        let Some(id) = &payment.bank_receipt_asset_id else { return Ok(None) };
        let asset = self
            .db
            .file_assets()
            .find_by_id(id.as_ref(), executor)
            .await?
            .ok_or_else(|| Error::ConflictError("付款申请实际付款回单不存在".into()))?;
        Ok(Some(material_from_asset(&asset)))
    }
}

fn payment_matches(
    payment: &SupplierPayment,
    request: &PurchasePaymentRequest,
    accounts: &BTreeSet<&str>,
) -> bool {
    payment.payment_request_id.as_deref() == Some(request.base.id.as_str())
        && payment.supplier_id.as_ref() == request.supplier_id
        && payment.payer_party_id.as_deref() == Some(request.payer_party_id.as_str())
        && !payment.request_allocations.is_empty()
        && payment.request_allocations.iter().all(|line| accounts.contains(line.payable_account_id.as_str()))
}
