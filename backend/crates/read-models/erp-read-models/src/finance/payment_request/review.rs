//! 详情专用审核依据；复用本申请来源授权，不授予其他申请整单或付款回单资格。

use std::collections::BTreeSet;

use erp_core::ids::{PayableAccountId, PayableEntryId, SupplierPaymentId};
use erp_finance::entity::payable::PurchasePaymentRequest;
use erp_finance::repository::prelude::*;
use erp_finance::repository::{PayableExt, PurchasePaymentRequestExt};
use persistence_core::Executor;

use super::{
    PaymentRequestReviewLine, PaymentReviewReservation, PaymentReviewSettlement,
    PurchasePaymentRequestReadService,
};
use crate::{Error, Result};

impl PurchasePaymentRequestReadService {
    // 处理付款申请审核时，读取每张已授权采购的核销与其他申请占用，不扩展文件下载范围。
    //
    // 调用参数：
    // * `request` / `executor` - 已授权申请及同一读取事务。
    //
    // 返回逐采购审核上下文。
    //
    // 来源记录缺失、主体不符或金额异常时拒绝。
    pub(super) async fn review_lines(
        &self,
        request: &PurchasePaymentRequest,
        executor: &mut dyn Executor,
    ) -> Result<Vec<PaymentRequestReviewLine>> {
        let mut rows = Vec::new();
        for line in &request.lines {
            let entries = self
                .db
                .payable_entries()
                .find_entries_by_account(&PayableAccountId::new(&line.payable_account_id), executor)
                .await?;
            let ids: Vec<_> = entries.iter().map(|entry| PayableEntryId::new(&entry.base.id)).collect();
            let allocations =
                self.db.payment_allocations().find_allocations_by_entries(&ids, executor).await?;
            let scope = entries.iter().map(|entry| entry.base.id.clone()).collect();
            let mut settlements = PaymentReviewSettlement::from_allocations(&allocations, &scope)?;
            self.settlement_labels(request, &mut settlements, executor).await?;

            let mut other_requests: Vec<_> = self
                .db
                .purchase_payment_requests()
                .reserved_for_account(&line.payable_account_id, executor)
                .await?
                .iter()
                .filter_map(|other| {
                    PaymentReviewReservation::from_request(other, request, &line.payable_account_id)
                })
                .collect();
            other_requests.sort_by(|left, right| {
                right.created_at.cmp(&left.created_at).then(left.request_id.cmp(&right.request_id))
            });

            let due_dates =
                entries.iter().map(|entry| entry.due_date).collect::<BTreeSet<_>>().into_iter().collect();
            rows.push(PaymentRequestReviewLine {
                payable_account_id: line.payable_account_id.clone(),
                due_dates,
                settlements,
                other_requests,
            });
        }
        Ok(rows)
    }

    // 补齐付款申请审核的展示名称时，批量补齐银行付款单号，金额始终使用本采购核销净额。
    async fn settlement_labels(
        &self,
        request: &PurchasePaymentRequest,
        rows: &mut [PaymentReviewSettlement],
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let ids = rows
            .iter()
            .filter_map(|row| row.payment_id.as_deref())
            .map(SupplierPaymentId::new)
            .collect::<Vec<_>>();
        let payments = self.db.supplier_payments().find_supplier_payments_by_ids(&ids, executor).await?;
        for row in rows.iter_mut() {
            let Some(id) = row.payment_id.as_deref() else { continue };
            let payment = payments
                .iter()
                .find(|payment| payment.base.id == id)
                .ok_or_else(|| Error::ConflictError("采购核销关联付款不存在".into()))?;
            if payment.supplier_id.as_ref() != request.supplier_id
                || payment.payer_party_id.as_deref() != Some(request.payer_party_id.as_str())
            {
                return Err(Error::ConflictError("采购历史付款主体不一致".into()));
            }
            row.document_no = Some(payment.payment_no.clone());
            row.occurred_at = payment.paid_at;
        }

        rows.sort_by(|left, right| right.occurred_at.cmp(&left.occurred_at).then(left.id.cmp(&right.id)));
        Ok(())
    }
}
