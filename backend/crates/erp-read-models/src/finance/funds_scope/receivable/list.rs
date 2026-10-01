//! 应收数据库页与全范围轻量金额摘要的展示组装。

use std::collections::HashMap;

use erp_finance::dto::receivable::ReceivableAccountListQuery;
use erp_finance::repository::receivable::ReceivableAccountRow;
use persistence_core::Executor;

use super::super::authorization::*;
use super::super::repository::accounts::{AccountPageRow, AccountSummaryRow};
use super::super::rows::{FundsSummaryView, ScopedReceivableAccountRow, build_summary, zero_amount};
use super::facts::ReceivableFacts;
use crate::Result;

impl FundsAccess {
    /// 组织条件在本次事务展开；负责人和登记经办人仍分别匹配。
    pub(super) async fn receivable_condition(
        &self,
        query: &ReceivableAccountListQuery,
        executor: &mut dyn Executor,
    ) -> Result<FundsLinkedCondition> {
        let org_unit_ids = match &query.org_unit_ids {
            Some(ids) => Some(
                self.expand_org_units(ids.as_slice(), query.include_descendants.unwrap_or(false), executor)
                    .await?
                    .into_iter()
                    .collect(),
            ),
            None => None,
        };
        Ok(FundsLinkedCondition {
            owner_user_ids: query.sales_owner_user_ids.as_ref().map(|ids| ids.as_slice().to_vec()),
            operator_user_ids: query.operator_user_ids.as_ref().map(|ids| ids.as_slice().to_vec()),
            secondary_operator_user_ids: None,
            org_unit_ids,
        })
    }
}

/// 当前页复用净核销与分录；真实来源只包含页面必要责任字段。
pub(super) fn receivable_page_rows(
    rows: &[AccountPageRow<ReceivableAccountRow>],
    facts: &mut ReceivableFacts,
) -> Vec<ScopedReceivableAccountRow> {
    rows.iter()
        .map(|item| {
            let row = &item.row;
            let source = &item.source;
            ScopedReceivableAccountRow {
                id: row.id.clone(),
                sales_order_id: source.id.clone(),
                sales_order_no: source.document_no.clone(),
                account_seq: row.account_seq,
                status: row.stable.status(),
                created_at: row.created_at,
                version: row.version,
                customer_id: row.customer_id.clone(),
                customer_name: None,
                counterparty_party_id: row.counterparty_party_id.clone(),
                counterparty_party_name: None,
                visible_settled_share: facts.share(&row.id),
                gross_total: Some(row.gross_total),
                settled_total: Some(row.settled_total),
                open_total: Some(row.open_total),
                open_invoiceable_total: Some(row.open_invoiceable_total),
                permission_limited: false,
                sales_owner_user_id: source.owner_user_id.clone(),
                business_org_unit_id: Some(source.business_org_unit_id.clone()),
                entries: facts.take_entries(&row.id),
            }
        })
        .collect()
}

/// 全范围使用数据库窄摘要和同拍净核销，保留原逐行加总顺序。
pub(super) fn receivable_summary(
    rows: &[AccountSummaryRow],
    facts: &ReceivableFacts,
    version: &str,
) -> Result<FundsSummaryView> {
    let mut triples = Vec::with_capacity(rows.len());
    let mut whole_sum = zero_amount();
    let mut owner_of = HashMap::new();
    for row in rows {
        triples.push((row.id.clone(), facts.share(&row.id), Some(row.order_id.clone())));
        whole_sum = whole_sum.checked_add(row.gross_total);
        if let Some(owner) = &row.owner_user_id {
            owner_of.insert(row.order_id.clone(), owner.clone());
        }
    }
    build_summary(&triples, &owner_of, Some(whole_sum), version, false)
}
