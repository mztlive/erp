//! 当前变化只形成明确阻断，不替换提交时的采购、销售和银行账户依据。

use erp_core::common::time::BusinessDate;
use erp_core::money::Amount;
use erp_finance::entity::payable::{
    PayableAccount, PaymentRequestSourceSnapshot, PurchasePaymentRequest, PurchasePaymentRequestLine,
};
use erp_party::PartyExt;
use erp_party::repository::prelude::*;
use erp_procurement::entity::purchase_order::{PurchaseOrder, PurchaseOrderStatus};
use erp_sales::repository::SalesOrderExt;
use persistence_core::Executor;

use super::PurchasePaymentRequestReadService;
use crate::Result;
use crate::purchase_center::source_impact::payment_paused_by_source_update;

impl PurchasePaymentRequestReadService {
    pub(super) async fn bank_account_changed(
        &self,
        request: &PurchasePaymentRequest,
        executor: &mut dyn Executor,
    ) -> Result<bool> {
        let Some(snapshot) = &request.payee_account else { return Ok(false) };
        let Some(account) = self.db.party_bank_accounts().find_by_id(&snapshot.account_id, executor).await?
        else {
            return Ok(true);
        };
        if account.base.version != snapshot.version || account.party_id.as_ref() != snapshot.supplier_party_id
        {
            return Ok(true);
        }
        Ok(!self
            .db
            .party_bank_accounts()
            .list_current_on(&account.party_id, BusinessDate::today(), executor)
            .await?
            .iter()
            .any(|candidate| candidate.base.id == snapshot.account_id))
    }

    pub(super) async fn source_changed(
        &self,
        request: &PurchasePaymentRequest,
        line: &PurchasePaymentRequestLine,
        order: &PurchaseOrder,
        account: &PayableAccount,
        executor: &mut dyn Executor,
    ) -> Result<bool> {
        if order.stable.current_revision_id.as_deref() != Some(line.purchase_revision_id.as_str())
            || account.purchase_revision_id.as_deref() != Some(line.purchase_revision_id.as_str())
            || subject_changed(
                &request.payer_party_id,
                &line.source_snapshot,
                account.settlement_party_id.as_deref(),
                account.settlement_party_name.as_deref(),
            )
            || account.gross_total != line.gross_total
            || account.open_total < Amount::zero()
            || account.payment_request_reserved_total > account.open_total
            || !matches!(
                order.stable.status,
                PurchaseOrderStatus::Effective
                    | PurchaseOrderStatus::PartiallyExecuted
                    | PurchaseOrderStatus::Completed
            )
        {
            return Ok(true);
        }
        for source in &line.source_snapshot.sales_sources {
            let current = self.db.sales_orders().find_by_id(&source.sales_order_id, executor).await?;
            let Some(current) = current else {
                return Ok(true);
            };
            if current.stable.current_revision_id.as_deref() != Some(source.sales_revision_id.as_str())
                && payment_paused_by_source_update(&self.db, &order.base.id, executor).await?
            {
                return Ok(true);
            }
        }
        Ok(false)
    }
}

fn subject_changed(
    payer_id: &str,
    frozen: &PaymentRequestSourceSnapshot,
    account_payer_id: Option<&str>,
    account_payer_name: Option<&str>,
) -> bool {
    // 名称证明属于各笔冻结来源；同一主体更名不要求历史名称与申请表头统一。
    account_payer_id != Some(payer_id)
        || frozen.sales_sources.is_empty()
        || frozen.sales_sources.iter().any(|source| {
            source.payer_party_id != payer_id || account_payer_name != Some(source.payer_party_name.as_str())
        })
}

#[cfg(test)]
mod tests {
    use erp_finance::entity::payable::PaymentRequestSalesSource;
    use serde_json::json;

    use super::*;

    fn source(name: &str) -> PaymentRequestSourceSnapshot {
        PaymentRequestSourceSnapshot {
            purchase_preview: json!({}),
            sales_sources: vec![PaymentRequestSalesSource {
                sales_order_id: "sales-1".into(),
                sales_revision_id: "sales-v1".into(),
                customer_id: "customer-1".into(),
                payer_party_id: "company-a".into(),
                payer_party_name: name.into(),
                preview: json!({}),
            }],
            allowed_attachment_ids: vec![],
        }
    }

    #[test]
    fn each_payable_matches_its_own_historical_name_for_one_company() {
        for name in ["我方公司原名称", "我方公司新名称"] {
            assert!(!subject_changed("company-a", &source(name), Some("company-a"), Some(name)));
        }
    }

    #[test]
    fn mismatched_company_or_own_frozen_name_blocks_source() {
        let frozen = source("我方公司原名称");
        assert!(subject_changed("company-b", &frozen, Some("company-b"), Some("我方公司原名称")));
        assert!(subject_changed("company-a", &frozen, Some("company-b"), Some("我方公司原名称")));
        assert!(subject_changed("company-a", &frozen, Some("company-a"), Some("我方公司新名称")));
        assert!(subject_changed("company-a", &frozen, Some("company-a"), None));
        let mut missing = frozen;
        missing.sales_sources.clear();
        assert!(subject_changed("company-a", &missing, Some("company-a"), Some("我方公司原名称")));
    }
}
