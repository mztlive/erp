//! 仅为已授权应付行批量读取当前采购修订，沿用调用方事务快照。

use std::collections::HashMap;

use erp_core::ids::PurchaseOrderRevisionId;
use erp_core::money::Amount;
use erp_finance::dto::payable::PaymentGuidanceView;
use erp_finance::entity::payable::PayableSourceType;
use erp_procurement::entity::purchase_order::PaymentTermSnapshot;
use erp_procurement::repository::PurchaseOrderExt;
use erp_supplier::{SupplierPaymentTerm, split_encoded_payment_term_snapshot};
use persistence_core::Executor;

use super::{FundsAccess, ScopedPayableAccountRow};
use crate::Result;

impl FundsAccess {
    /// 为整单可读的采购应付补齐付款依据；缺失版本保留空值。
    ///
    /// # 参数
    /// * `rows` - 已完成范围裁剪的当前页或详情。
    /// * `executor` - 授权读取使用的同一执行器。
    ///
    /// # 返回
    /// 原位填充建议；不读取供应商当前商务条款。
    ///
    /// # 错误
    /// 仓储读取失败时返回错误。
    pub(super) async fn fill_payment_guidance(
        &self,
        rows: &mut [ScopedPayableAccountRow],
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let ids = rows
            .iter()
            .filter(|row| !row.permission_limited && row.source_type == PayableSourceType::PurchaseOrder)
            .map(|row| row.source_document_id.clone())
            .collect::<Vec<_>>();
        if ids.is_empty() {
            return Ok(());
        }
        let orders = self.db.purchase_order().find_orders_by_ids(&ids, executor).await?;
        let revision_ids = orders
            .iter()
            .filter_map(|order| order.stable.current_revision_id.clone())
            .map(PurchaseOrderRevisionId::new)
            .collect::<Vec<_>>();
        let revisions = self
            .db
            .purchase_order()
            .find_revisions_by_ids(&revision_ids, executor)
            .await?
            .into_iter()
            .map(|revision| (revision.base.id.clone(), revision))
            .collect::<HashMap<_, _>>();
        let by_order = orders
            .iter()
            .filter_map(|order| {
                let revision = revisions.get(order.stable.current_revision_id.as_ref()?)?;
                (revision.purchase_order_id.as_ref() == order.base.id)
                    .then_some((order.base.id.as_str(), revision))
            })
            .collect::<HashMap<_, _>>();
        for row in rows {
            if !row.permission_limited && row.source_type == PayableSourceType::PurchaseOrder {
                row.payment_guidance = by_order.get(row.source_document_id.as_str()).and_then(|revision| {
                    Some(guidance(
                        &revision.payment_term_snapshot,
                        revision.gross_amount,
                        row.settled_total?,
                        row.open_total?,
                    ))
                });
            }
        }
        Ok(())
    }
}

fn guidance(
    snapshot: &PaymentTermSnapshot,
    gross: Amount,
    paid: Amount,
    open: Amount,
) -> PaymentGuidanceView {
    let code = split_encoded_payment_term_snapshot(&snapshot.payment_term_code).payment_term_code;
    let term = SupplierPaymentTerm::parse(&code).ok();
    let requirement = snapshot.required_prepayment(gross);
    let valid = term.is_some_and(|term| term.prepay_gate() == snapshot.prepay_gate) && requirement.is_ok();
    let mut view = PaymentGuidanceView::from_requirement(
        term.map(SupplierPaymentTerm::label).unwrap_or_else(|| "付款条件待核对".into()),
        gross,
        paid,
        open,
        snapshot.prepay_gate,
        requirement.ok().flatten(),
    );
    if !valid {
        view.suggested_amount = None;
    }
    view
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot(code: &str, gate: bool, ratio: Option<&str>) -> PaymentTermSnapshot {
        PaymentTermSnapshot {
            payment_term_code: code.into(),
            prepay_gate: gate,
            prepay_minimum_amount: None,
            prepay_minimum_ratio: ratio.map(|r| r.parse().unwrap()),
        }
    }

    #[test]
    fn frozen_ratio_and_effective_net_payment_control_suggestion() {
        let term = snapshot("PREPAY_50", true, Some("0.5"));
        let result = guidance(&term, "60".parse().unwrap(), "10".parse().unwrap(), "50".parse().unwrap());
        assert_eq!(result.term_label, "先款 50%");
        assert_eq!(result.required_prepayment, Some("30".parse().unwrap()));
        assert_eq!(result.suggested_amount, Some("20".parse().unwrap()));
        let changed = guidance(&term, "100".parse().unwrap(), "10".parse().unwrap(), "90".parse().unwrap());
        assert_eq!(changed.suggested_amount, Some("40".parse().unwrap()));
    }

    #[test]
    fn missing_unknown_or_inconsistent_terms_never_suggest_full_payment() {
        for term in [
            snapshot("PREPAY_50", true, None),
            snapshot("unknown", false, None),
            snapshot("PREPAY_50", false, None),
        ] {
            let result = guidance(&term, "60".parse().unwrap(), Amount::zero(), "60".parse().unwrap());
            assert_eq!(result.suggested_amount, None);
        }
    }

    #[test]
    fn satisfied_prepayment_has_no_implicit_tail_schedule() {
        let term = snapshot("PREPAY_50", true, Some("0.5"));
        let result = guidance(&term, "60".parse().unwrap(), "30".parse().unwrap(), "30".parse().unwrap());
        assert_eq!(result.prepayment_gap, Some(Amount::zero()));
        assert_eq!(result.suggested_amount, None);
    }
}
