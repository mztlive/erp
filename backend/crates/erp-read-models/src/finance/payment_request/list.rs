//! 授权后的列表分页；姓名筛选与排序在截取当前页之前执行。

use std::cmp::Ordering;
use std::collections::HashMap;

use application_core::PageView;
use erp_core::money::Amount;
use erp_finance::dto::payable::payment_request::{PaymentRequestSort, PurchasePaymentRequestQuery};
use erp_finance::entity::payable::PurchasePaymentRequest;

use crate::{Error, Result};

pub(super) struct PaymentRequestList;

impl PaymentRequestList {
    /// 在完整授权结果上筛选姓名、稳定排序并截取当前页。
    ///
    /// # 参数
    /// * `rows` / `names` - 已授权申请及同一事务读取的申请人姓名。
    /// * `query` - 规范化后的筛选、排序与分页条件。
    /// # 返回
    /// 返回当前页及筛选后的准确总数。
    /// # 错误
    /// 总数无法表示为接口整数时返回错误。
    pub(super) fn page(
        mut rows: Vec<PurchasePaymentRequest>,
        names: &HashMap<String, String>,
        query: &PurchasePaymentRequestQuery,
    ) -> Result<PageView<PurchasePaymentRequest>> {
        if let Some(name) = &query.applicant_name {
            let needle = name.to_lowercase();
            rows.retain(|row| applicant(row, names).to_lowercase().contains(&needle));
        }
        if let Some(sort) = query.sort_by {
            rows.sort_by(|left, right| {
                let order = compare(left, right, names, sort);
                let order = if query.sort_desc.unwrap_or(false) { order.reverse() } else { order };
                order
                    .then_with(|| right.base.created_at.cmp(&left.base.created_at))
                    .then_with(|| right.base.id.cmp(&left.base.id))
            });
        }
        let total = i64::try_from(rows.len()).map_err(|_| Error::Internal("付款申请总数超过范围".into()))?;
        let page = query.page.unwrap_or(1).max(1);
        let page_size = query.page_size.unwrap_or(20).clamp(1, 100);
        let start = page.saturating_sub(1).saturating_mul(u64::from(page_size));
        let start = usize::try_from(start).unwrap_or(usize::MAX);
        let items = rows.into_iter().skip(start).take(usize::try_from(page_size).unwrap_or(100)).collect();
        Ok(PageView { items, total, page, page_size })
    }
}

fn applicant<'a>(row: &PurchasePaymentRequest, names: &'a HashMap<String, String>) -> &'a str {
    names.get(&row.created_by).map(String::as_str).unwrap_or("未记录姓名")
}

fn supplier(row: &PurchasePaymentRequest) -> &str {
    row.lines
        .first()
        .and_then(|line| line.source_snapshot.purchase_preview.get("supplier_name"))
        .and_then(|value| value.as_str())
        .unwrap_or(&row.supplier_id)
}

fn paid(row: &PurchasePaymentRequest) -> Amount {
    row.lines.iter().fold(Amount::zero(), |total, line| total.checked_add(line.paid_amount))
}

fn unpaid(row: &PurchasePaymentRequest) -> Amount {
    row.lines.iter().fold(row.total_amount, |total, line| {
        total.checked_sub(line.paid_amount).checked_sub(line.closed_amount)
    })
}

fn compare(
    left: &PurchasePaymentRequest,
    right: &PurchasePaymentRequest,
    names: &HashMap<String, String>,
    sort: PaymentRequestSort,
) -> Ordering {
    match sort {
        PaymentRequestSort::RequestNo => left.request_no.cmp(&right.request_no),
        PaymentRequestSort::Supplier => supplier(left).cmp(supplier(right)),
        PaymentRequestSort::Applicant => applicant(left, names).cmp(applicant(right, names)),
        PaymentRequestSort::Amount => left.total_amount.cmp(&right.total_amount),
        PaymentRequestSort::Paid => paid(left).cmp(&paid(right)),
        PaymentRequestSort::Unpaid => unpaid(left).cmp(&unpaid(right)),
        PaymentRequestSort::Date => left.data.desired_payment_date.cmp(&right.data.desired_payment_date),
        PaymentRequestSort::Status => left.status.as_str().cmp(right.status.as_str()),
    }
}

#[cfg(test)]
mod tests {
    use erp_core::common::time::BusinessDate;
    use erp_finance::entity::payable::{
        PaymentRequestLineData, PaymentRequestSourceSnapshot, PurchasePaymentRequestData,
        PurchasePaymentRequestLine,
    };
    use serde_json::json;

    use super::*;

    fn request(id: &str, author: &str, amount: &str) -> PurchasePaymentRequest {
        let amount: Amount = amount.parse().unwrap();
        let mut row = PurchasePaymentRequest::new(
            id.into(),
            PurchasePaymentRequestData {
                supplier_id: "supplier".into(),
                payee_bank_account_id: "bank".into(),
                lines: vec![PaymentRequestLineData { payable_account_id: "account".into(), amount }],
                desired_payment_date: BusinessDate::from_ymd(2026, 10, 15).unwrap(),
                reason: "付款".into(),
                attachment_ids: vec![],
            },
            author,
        )
        .unwrap();
        row.base.id = id.into();
        row.base.created_at = 1;
        row.lines = vec![PurchasePaymentRequestLine {
            payable_account_id: "account".into(),
            purchase_order_id: "purchase".into(),
            purchase_revision_id: "revision".into(),
            requested_amount: amount,
            paid_amount: Amount::zero(),
            closed_amount: Amount::zero(),
            gross_total: amount,
            settled_total: Amount::zero(),
            open_total: amount,
            other_reserved_total: Amount::zero(),
            payment_terms: json!({}),
            source_snapshot: PaymentRequestSourceSnapshot {
                purchase_preview: json!({"supplier_name": "供应商"}),
                sales_sources: vec![],
                allowed_attachment_ids: vec![],
            },
        }];
        row
    }

    #[test]
    fn applicant_filter_and_global_numeric_sort_precede_pagination() {
        let rows = vec![
            request("a", "buyer", "100"),
            request("b", "other", "1"),
            request("c", "buyer", "20"),
            request("d", "buyer", "3"),
        ];
        let names = HashMap::from([("buyer".into(), "陈Alice".into()), ("other".into(), "其他".into())]);
        let mut query = PurchasePaymentRequestQuery {
            applicant_name: Some(" ALICE ".into()),
            sort_by: Some(PaymentRequestSort::Amount),
            page_size: Some(2),
            ..Default::default()
        }
        .normalize()
        .unwrap();
        let first = PaymentRequestList::page(rows.clone(), &names, &query).unwrap();
        assert_eq!(first.total, 3);
        assert_eq!(first.items.iter().map(|r| r.base.id.as_str()).collect::<Vec<_>>(), ["d", "c"]);
        query.page = Some(2);
        let last = PaymentRequestList::page(rows.clone(), &names, &query).unwrap();
        assert_eq!(last.total, 3);
        assert_eq!(last.items[0].base.id, "a");
        query.page = Some(u64::MAX);
        let beyond = PaymentRequestList::page(rows, &names, &query).unwrap();
        assert!(beyond.items.is_empty());
        assert_eq!(beyond.total, 3);
    }

    #[test]
    fn stable_ties_descending_sort_empty_results_and_missing_names() {
        let rows =
            vec![request("a", "buyer", "10"), request("c", "buyer", "20"), request("b", "buyer", "10")];
        let mut query = PurchasePaymentRequestQuery {
            sort_by: Some(PaymentRequestSort::Amount),
            sort_desc: Some(true),
            ..Default::default()
        };
        let page = PaymentRequestList::page(rows.clone(), &HashMap::new(), &query).unwrap();
        assert_eq!(page.items.iter().map(|r| r.base.id.as_str()).collect::<Vec<_>>(), ["c", "b", "a"]);
        query.applicant_name = Some("不存在".into());
        let empty = PaymentRequestList::page(rows.clone(), &HashMap::new(), &query).unwrap();
        assert_eq!(empty.total, 0);
        assert!(empty.items.is_empty());
        query.applicant_name = Some("未记录姓名".into());
        assert_eq!(PaymentRequestList::page(rows, &HashMap::new(), &query).unwrap().total, 3);
    }

    #[test]
    fn monetary_sort_uses_net_paid_and_deducts_closed_amount() {
        let mut left = request("a", "buyer", "100");
        left.lines[0].paid_amount = "30".parse().unwrap();
        left.lines[0].closed_amount = "60".parse().unwrap();
        let mut right = request("b", "buyer", "20");
        right.lines[0].paid_amount = "5".parse().unwrap();
        assert_eq!(paid(&left), "30".parse().unwrap());
        assert_eq!(unpaid(&left), "10".parse().unwrap());
        assert_eq!(compare(&left, &right, &HashMap::new(), PaymentRequestSort::Paid), Ordering::Greater);
        assert_eq!(compare(&left, &right, &HashMap::new(), PaymentRequestSort::Unpaid), Ordering::Less);
    }
}
