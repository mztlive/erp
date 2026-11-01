//! 冻结预览按精确来源身份校验，同一销售同一版本仅展示一次。

use std::collections::BTreeMap;

use erp_finance::entity::payable::PurchasePaymentRequestLine;
use serde_json::Value;

use crate::{Error, Result};

pub(super) fn frozen_previews(
    lines: &[PurchasePaymentRequestLine],
    payer: &str,
) -> Result<(Vec<Value>, Vec<Value>)> {
    let mut sales = BTreeMap::new();
    let mut purchases = Vec::new();
    for line in lines {
        let preview = &line.source_snapshot.purchase_preview;
        if preview.get("purchase_order_id").and_then(Value::as_str) != Some(line.purchase_order_id.as_str())
            || preview.get("revision_id").and_then(Value::as_str) != Some(line.purchase_revision_id.as_str())
        {
            return Err(Error::ConflictError("付款申请冻结采购预览与精确来源不一致".into()));
        }
        purchases.push(preview.clone());
        for source in &line.source_snapshot.sales_sources {
            if source.payer_party_id != payer
                || source.preview.get("sales_order_id").and_then(Value::as_str)
                    != Some(source.sales_order_id.as_str())
                || source.preview.get("revision_id").and_then(Value::as_str)
                    != Some(source.sales_revision_id.as_str())
            {
                return Err(Error::ConflictError("付款申请冻结销售预览与精确来源不一致".into()));
            }
            let key = (source.sales_order_id.clone(), source.sales_revision_id.clone());
            if let Some(previous) = sales.insert(key, source.preview.clone())
                && previous != source.preview
            {
                return Err(Error::ConflictError("付款申请同一销售版本冻结材料不一致".into()));
            }
        }
    }
    Ok((purchases, sales.into_values().collect()))
}

#[cfg(test)]
mod tests {
    use erp_core::money::Amount;
    use erp_finance::entity::payable::{PaymentRequestSalesSource, PaymentRequestSourceSnapshot};
    use serde_json::json;

    use super::*;

    fn line(purchase: &str, revision: &str) -> PurchasePaymentRequestLine {
        PurchasePaymentRequestLine {
            payable_account_id: format!("account-{purchase}"),
            purchase_order_id: purchase.into(),
            purchase_revision_id: format!("revision-{purchase}"),
            requested_amount: "10".parse().unwrap(),
            paid_amount: Amount::zero(),
            closed_amount: Amount::zero(),
            gross_total: "20".parse().unwrap(),
            settled_total: Amount::zero(),
            open_total: "20".parse().unwrap(),
            other_reserved_total: Amount::zero(),
            payment_terms: json!({}),
            source_snapshot: PaymentRequestSourceSnapshot {
                purchase_preview: json!({"purchase_order_id":purchase,"revision_id":format!("revision-{purchase}")}),
                sales_sources: vec![PaymentRequestSalesSource {
                    sales_order_id: "sales".into(),
                    sales_revision_id: revision.into(),
                    customer_id: "customer".into(),
                    payer_party_id: "payer".into(),
                    payer_party_name: "我方主体".into(),
                    preview: json!({"sales_order_id":"sales","revision_id":revision,"totals":{"gross":"30"}}),
                }],
                allowed_attachment_ids: vec![],
            },
        }
    }

    #[test]
    fn same_sales_revision_has_one_preview_and_different_revision_keeps_separate_previews() {
        let (purchases, sales) =
            frozen_previews(&[line("p-1", "v-1"), line("p-2", "v-1"), line("p-3", "v-2")], "payer").unwrap();
        assert_eq!(purchases.len(), 3);
        assert_eq!(sales.len(), 2);
        assert_eq!(sales[0]["revision_id"], "v-1");
        assert_eq!(sales[1]["revision_id"], "v-2");
    }

    #[test]
    fn previews_reject_cross_request_purchase_or_sales_versions_and_changed_same_version() {
        let mut wrong = line("p-1", "v-1");
        wrong.source_snapshot.purchase_preview["revision_id"] = json!("other");
        assert!(matches!(frozen_previews(&[wrong], "payer"), Err(Error::ConflictError(_))));
        let mut changed = line("p-2", "v-1");
        changed.source_snapshot.sales_sources[0].preview["totals"]["gross"] = json!("100");
        assert!(matches!(
            frozen_previews(&[line("p-1", "v-1"), changed], "payer"),
            Err(Error::ConflictError(_))
        ));
        assert!(matches!(
            frozen_previews(&[line("p-1", "v-1")], "other-payer"),
            Err(Error::ConflictError(_))
        ));
        assert_eq!(frozen_previews(&[], "payer").unwrap(), (vec![], vec![]));
    }
}
