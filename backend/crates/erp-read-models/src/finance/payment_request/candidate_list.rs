//! 可申请采购单的独立条件、全局排序、分页和跨页选择身份。

use std::cmp::Ordering;

use application_core::{AuditActor, PageView};
use serde::{Deserialize, Serialize};

use super::{PurchasePaymentRequestCandidate, PurchasePaymentRequestReadService};
use crate::{Error, Result};

/// 可申请列表查询；身份及文本条件同时满足，分页不改变来源资格。
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PaymentCandidateQuery {
    pub purchase_order_no: Option<String>,
    pub purchase_summary: Option<String>,
    pub supplier_id: Option<String>,
    pub payer_party_id: Option<String>,
    pub supplier_name: Option<String>,
    pub payer_party_name: Option<String>,
    pub page: Option<u64>,
    pub page_size: Option<u32>,
    pub sort_by: Option<PaymentCandidateSort>,
    pub sort_desc: Option<bool>,
}

/// 与可申请列表列对应的排序白名单。
#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PaymentCandidateSort {
    PurchaseOrderNo,
    SupplierName,
    PaymentTermName,
    TotalAmount,
    PaidAmount,
    ReservedAmount,
    AvailableAmount,
}

impl PaymentCandidateQuery {
    fn normalize(mut self) -> Result<Self> {
        for text in [
            &mut self.supplier_id,
            &mut self.payer_party_id,
            &mut self.purchase_order_no,
            &mut self.purchase_summary,
            &mut self.supplier_name,
            &mut self.payer_party_name,
        ] {
            *text = text.take().map(|value| value.trim().to_owned()).filter(|value| !value.is_empty());
            if text.as_ref().is_some_and(|value| value.chars().count() > 200) {
                return Err(Error::ValidationError("查询条件不能超过 200 个字符".into()));
            }
        }
        self.page = Some(self.page.unwrap_or(1).max(1));
        self.page_size = Some(self.page_size.unwrap_or(50).clamp(1, 100));
        Ok(self)
    }

    fn matches(&self, row: &PurchasePaymentRequestCandidate) -> bool {
        if self.supplier_id.as_ref().is_some_and(|id| id != &row.supplier_id)
            || self.payer_party_id.as_ref().is_some_and(|id| id != &row.payer_party_id)
        {
            return false;
        }
        [
            (&self.purchase_order_no, row.purchase_no.as_str()),
            (&self.purchase_summary, row.purchase_summary.as_str()),
            (&self.supplier_name, row.supplier_name.as_str()),
            (&self.payer_party_name, row.payer_party_name.as_str()),
        ]
        .into_iter()
        .all(|(filter, value)| {
            filter.as_ref().is_none_or(|needle| value.to_lowercase().contains(&needle.to_lowercase()))
        })
    }

    fn matching(
        &self,
        mut rows: Vec<PurchasePaymentRequestCandidate>,
    ) -> Vec<PurchasePaymentRequestCandidate> {
        rows.retain(|row| self.matches(row));
        rows.sort_by(|left, right| {
            let order = self.sort_by.map_or_else(
                || right.purchase_no.cmp(&left.purchase_no),
                |sort| {
                    let order = compare(left, right, sort);
                    if self.sort_desc.unwrap_or(false) { order.reverse() } else { order }
                },
            );
            order.then_with(|| left.payable_account_id.cmp(&right.payable_account_id))
        });
        rows
    }

    fn page(
        &self,
        rows: Vec<PurchasePaymentRequestCandidate>,
    ) -> Result<PageView<PurchasePaymentRequestCandidate>> {
        let rows = self.matching(rows);
        let total =
            i64::try_from(rows.len()).map_err(|_| Error::Internal("可申请采购单总数超过范围".into()))?;
        let page = self.page.unwrap_or(1).max(1);
        let page_size = self.page_size.unwrap_or(50).clamp(1, 100);
        let offset = page.saturating_sub(1).saturating_mul(u64::from(page_size));
        let items = rows
            .into_iter()
            .skip(usize::try_from(offset).unwrap_or(usize::MAX))
            .take(usize::try_from(page_size).unwrap_or(100))
            .collect();
        Ok(PageView { items, total, page, page_size })
    }

    fn ids(&self, rows: Vec<PurchasePaymentRequestCandidate>) -> Vec<String> {
        self.matching(rows).into_iter().map(|row| row.payable_account_id).collect()
    }
}

impl PurchasePaymentRequestReadService {
    /// 查询可申请采购单的当前页及同一条件下的准确总数。
    ///
    /// # 参数
    /// * `actor` / `query` - 当前账号与独立筛选、排序和分页。
    /// # 返回
    /// 返回资格、余额和全部条件验证后的当前页。
    /// # 错误
    /// 参数、来源资格或候选读取失败时拒绝。
    pub async fn candidate_page(
        &self,
        actor: &AuditActor,
        query: PaymentCandidateQuery,
    ) -> Result<PageView<PurchasePaymentRequestCandidate>> {
        let query = query.normalize()?;
        query.page(self.candidates(actor, query.supplier_id.as_deref()).await?)
    }

    /// 返回当前仍可申请且匹配全部条件的应付身份，用于跨页全选与发起前复验。
    ///
    /// # 参数
    /// * `actor` / `query` - 当前账号与独立筛选条件，忽略分页。
    /// # 返回
    /// 返回同一资格快照下全部匹配身份，不返回其他页面的采购详情。
    /// # 错误
    /// 参数、来源资格或候选读取失败时拒绝。
    pub async fn candidate_ids(
        &self,
        actor: &AuditActor,
        query: PaymentCandidateQuery,
    ) -> Result<Vec<String>> {
        let query = query.normalize()?;
        Ok(query.ids(self.candidates(actor, query.supplier_id.as_deref()).await?))
    }
}

fn term(row: &PurchasePaymentRequestCandidate) -> &str {
    row.payment_terms
        .get("payment_term_name")
        .or_else(|| row.payment_terms.get("payment_term_code"))
        .and_then(|value| value.as_str())
        .unwrap_or("")
}

fn compare(
    left: &PurchasePaymentRequestCandidate,
    right: &PurchasePaymentRequestCandidate,
    sort: PaymentCandidateSort,
) -> Ordering {
    match sort {
        PaymentCandidateSort::PurchaseOrderNo => left.purchase_no.cmp(&right.purchase_no),
        PaymentCandidateSort::SupplierName => left.supplier_name.cmp(&right.supplier_name),
        PaymentCandidateSort::PaymentTermName => term(left).cmp(term(right)),
        PaymentCandidateSort::TotalAmount => left.gross_total.cmp(&right.gross_total),
        PaymentCandidateSort::PaidAmount => left.settled_total.cmp(&right.settled_total),
        PaymentCandidateSort::ReservedAmount => left.reserved_total.cmp(&right.reserved_total),
        PaymentCandidateSort::AvailableAmount => left.available_total.cmp(&right.available_total),
    }
}

#[cfg(test)]
mod tests {
    use erp_core::money::Amount;
    use serde_json::json;

    use super::*;

    fn candidate(id: &str, amount: &str) -> PurchasePaymentRequestCandidate {
        let amount = amount.parse().unwrap();
        PurchasePaymentRequestCandidate {
            purchase_summary: "Tea 茶叶 A+B".into(),
            prepayment_gap: None,
            purchase_order_id: format!("purchase-{id}"),
            purchase_no: format!("CG-{id}"),
            purchase_revision_id: "revision".into(),
            supplier_id: "supplier".into(),
            supplier_name: "茶叶供应商".into(),
            payer_party_id: "payer".into(),
            payer_party_name: "福尚云".into(),
            payable_account_id: id.into(),
            gross_total: amount,
            settled_total: Amount::zero(),
            open_total: amount,
            reserved_total: Amount::zero(),
            available_total: amount,
            payment_terms: json!({"payment_term_name":"月结"}),
            bank_accounts: vec![],
        }
    }

    #[test]
    fn identity_filters_distinguish_same_names_and_apply_to_pages_and_selection() {
        let query = PaymentCandidateQuery {
            supplier_id: Some(" supplier ".into()),
            payer_party_id: Some(" payer ".into()),
            purchase_summary: Some("茶叶".into()),
            ..Default::default()
        }
        .normalize()
        .unwrap();
        let mut other_supplier = candidate("2", "10");
        other_supplier.supplier_id = "supplier-other".into();
        let mut other_payer = candidate("3", "10");
        other_payer.payer_party_id = "payer-other".into();
        let mut renamed = candidate("1", "10");
        renamed.supplier_name = "新供应商名称".into();
        renamed.payer_party_name = "新主体名称".into();
        let rows = vec![renamed, other_supplier, other_payer];
        let page = query.page(rows.clone()).unwrap();
        assert_eq!(page.total, 1);
        assert_eq!(page.items[0].payable_account_id, "1");
        assert_eq!(query.ids(rows.clone()), ["1"]);
        let cleared = PaymentCandidateQuery {
            supplier_id: Some(" ".into()),
            payer_party_id: Some("".into()),
            ..Default::default()
        }
        .normalize()
        .unwrap();
        assert_eq!(cleared.page(rows).unwrap().total, 3);
        assert!(
            PaymentCandidateQuery { payer_party_id: Some("x".repeat(201)), ..Default::default() }
                .normalize()
                .is_err()
        );
    }

    #[test]
    fn all_four_independent_filters_must_match_and_text_is_literal_case_insensitive() {
        let query = PaymentCandidateQuery {
            purchase_order_no: Some(" cg-1 ".into()),
            purchase_summary: Some("tea 茶叶 a+b".into()),
            supplier_name: Some("供应商".into()),
            payer_party_name: Some("福尚云".into()),
            ..Default::default()
        }
        .normalize()
        .unwrap();
        let row = candidate("1", "100");
        assert!(query.matches(&row));
        assert!(!query.matches(&candidate("2", "100")));
        let mut wrong_supplier = row.clone();
        wrong_supplier.supplier_name = "另一家公司".into();
        assert!(!query.matches(&wrong_supplier));
        let mut wrong_payer = row.clone();
        wrong_payer.payer_party_name = "其他主体".into();
        assert!(!query.matches(&wrong_payer));
        let mut wrong_summary = row;
        wrong_summary.purchase_summary = "Tea 茶叶 AB".into();
        assert!(!query.matches(&wrong_summary));
    }

    #[test]
    fn numeric_sort_and_total_precede_paging_and_selection_includes_every_page() {
        let rows = vec![candidate("1", "100"), candidate("2", "20"), candidate("3", "3")];
        let mut query = PaymentCandidateQuery {
            page_size: Some(2),
            sort_by: Some(PaymentCandidateSort::AvailableAmount),
            ..Default::default()
        }
        .normalize()
        .unwrap();
        let page = query.page(rows.clone()).unwrap();
        assert_eq!(page.total, 3);
        assert_eq!(
            page.items.iter().map(|row| row.payable_account_id.as_str()).collect::<Vec<_>>(),
            ["3", "2"]
        );
        query.page = Some(2);
        assert_eq!(query.page(rows.clone()).unwrap().items[0].payable_account_id, "1");
        assert_eq!(query.ids(rows.clone()), ["3", "2", "1"]);
        query.page = Some(u64::MAX);
        let beyond = query.page(rows.clone()).unwrap();
        assert_eq!(beyond.total, 3);
        assert!(beyond.items.is_empty());
        query.supplier_name = Some("不存在".into());
        assert_eq!(query.page(rows.clone()).unwrap().total, 0);
        assert!(query.ids(rows).is_empty());
    }

    #[test]
    fn normalization_bounds_input_and_sort_ties_are_stable() {
        let query = PaymentCandidateQuery {
            purchase_summary: Some("  ".into()),
            page: Some(0),
            page_size: Some(1000),
            ..Default::default()
        }
        .normalize()
        .unwrap();
        assert_eq!(query.purchase_summary, None);
        assert_eq!((query.page, query.page_size), (Some(1), Some(100)));
        assert!(
            PaymentCandidateQuery { supplier_name: Some("字".repeat(201)), ..Default::default() }
                .normalize()
                .is_err()
        );
        let query = PaymentCandidateQuery {
            sort_by: Some(PaymentCandidateSort::TotalAmount),
            sort_desc: Some(true),
            ..Default::default()
        }
        .normalize()
        .unwrap();
        assert_eq!(
            query.ids(vec![candidate("b", "10"), candidate("c", "20"), candidate("a", "10")]),
            ["c", "a", "b"]
        );
    }

    #[test]
    fn selection_only_returns_candidates_present_in_the_current_eligible_snapshot() {
        let query = PaymentCandidateQuery::default().normalize().unwrap();
        assert_eq!(query.ids(vec![candidate("eligible", "20")]), ["eligible"]);
        assert!(query.ids(vec![]).is_empty());
        let mut query = query;
        query.purchase_summary = Some("不存在".into());
        assert!(query.ids(vec![candidate("eligible", "20")]).is_empty());
    }
}
