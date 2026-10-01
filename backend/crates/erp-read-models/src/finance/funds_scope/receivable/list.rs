//! 应收列表本次快照的筛选、分页与汇总组装。

use std::collections::hash_map::DefaultHasher;
use std::collections::{BTreeSet, HashMap};
use std::hash::{Hash, Hasher};

use erp_core::ids::SalesOrderId;
use erp_finance::dto::receivable::{ReceivableAccountListQuery, SortDir};
use erp_finance::ports::funds_scope::FundsResolvedScope;
use erp_finance::repository::keyword::FinanceSearchTarget;
use erp_finance::repository::prelude::*;
use erp_finance::repository::receivable::ReceivableAccountRow;
use erp_finance::repository::{ReceivableAccountFilter, ReceivableExt};
use erp_sales::entity::sales_order::SalesOrder;
use erp_sales::repository::SalesOrderExt;
use erp_sales::repository::prelude::*;
use persistence_core::Executor;

use super::super::authorization::*;
use super::super::rows::{FundsSummaryView, ScopedReceivableAccountRow, build_summary, zero_amount};
use super::facts::ReceivableFacts;
use crate::finance::search::keyword_ids;
use crate::{Error, Result};

/// 已按授权和业务条件裁剪的行，保持仓储原排序及原版本指纹输入。
pub(super) struct ReceivableScopeRows {
    pub(super) rows: Vec<ReceivableAccountRow>,
    pub(super) orders: HashMap<String, SalesOrder>,
    fingerprint: DefaultHasher,
}

impl ReceivableScopeRows {
    /// 返回原行顺序与关联销售版本绑定的快照指纹。
    pub(super) fn version(&self) -> String {
        format!("{:x}", self.fingerprint.finish())
    }
}

impl FundsAccess {
    /// 使用同一执行器解析筛选、取候选及来源事实，并保留原授权判定。
    pub(super) async fn receivable_scope_rows(
        &self,
        query: &ReceivableAccountListQuery,
        access: &FundsResolvedScope,
        authorization: &FundsAuthorization,
        executor: &mut dyn Executor,
    ) -> Result<ReceivableScopeRows> {
        let condition = self.receivable_condition(query, executor).await?;
        let allowed = self
            .authorized_sales_ids(authorization, executor)
            .await?
            .map(|ids| ids.into_iter().collect::<BTreeSet<_>>());
        let candidates = self.receivable_candidates(query, executor).await?;
        let orders = self.receivable_order_map(&candidates, executor).await?;
        let mut fingerprint = DefaultHasher::new();
        authorization.context.scope_version.hash(&mut fingerprint);
        let rows =
            filter_receivable_rows(candidates, &orders, &allowed, access, &condition, &mut fingerprint)?;
        Ok(ReceivableScopeRows { rows, orders, fingerprint })
    }

    /// 组织条件在本次事务展开；负责人和登记经办人仍分别匹配。
    async fn receivable_condition(
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

    /// 沿用关键词关联和仓储排序，候选达到既有上限时整体拒绝。
    async fn receivable_candidates(
        &self,
        query: &ReceivableAccountListQuery,
        executor: &mut dyn Executor,
    ) -> Result<Vec<ReceivableAccountRow>> {
        let keyword_ids = keyword_ids(&self.db, query.q.as_deref(), FinanceSearchTarget::Receivable).await?;
        let filter = ReceivableAccountFilter {
            keyword_ids,
            keyword: None,
            keyword_sales_order_ids: Vec::new(),
            keyword_party_ids: Vec::new(),
            account_id: query.account_id.clone(),
            customer_id: query.customer_id.clone(),
            counterparty_party_id: query.counterparty_party_id.clone(),
            status: query.status,
            sales_order_id: query.sales_order_id.clone(),
            page: 1,
            page_size: 10_000,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: matches!(query.paging.sort_dir, SortDir::Asc),
        };
        let candidates = self.db.receivable_accounts().search_receivable_accounts(&filter, executor).await?;
        if candidates.items.len() >= 10_000 {
            return Err(Error::ValidationError("应收查询超过上限，请收窄组织或负责人条件".into()));
        }
        Ok(candidates.items)
    }

    /// 批量取回候选来源销售单；软删除或缺失来源仍在后续裁剪时跳过。
    async fn receivable_order_map(
        &self,
        rows: &[ReceivableAccountRow],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, SalesOrder>> {
        let ids = rows
            .iter()
            .map(|row| row.sales_order_id.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        let mut orders = HashMap::new();
        for chunk in ids.chunks(500) {
            let keys = chunk.iter().map(SalesOrderId::new).collect::<Vec<_>>();
            for order in self.db.sales_orders().find_orders_by_ids(&keys, executor).await? {
                orders.insert(order.base.id.clone(), order);
            }
        }
        Ok(orders)
    }
}

/// 筛选逐行保持原顺序，指纹仍依次绑定子账和来源销售单版本。
fn filter_receivable_rows(
    candidates: Vec<ReceivableAccountRow>,
    orders: &HashMap<String, SalesOrder>,
    allowed: &Option<BTreeSet<String>>,
    access: &FundsResolvedScope,
    condition: &FundsLinkedCondition,
    fingerprint: &mut DefaultHasher,
) -> Result<Vec<ReceivableAccountRow>> {
    let mut filtered = Vec::new();
    for row in candidates {
        let Some(order) = orders.get(&row.sales_order_id) else {
            continue;
        };
        if allowed.as_ref().is_some_and(|ids| !ids.contains(&order.base.id)) {
            continue;
        }
        let facts = FundsLinkedFacts {
            owner_user_id: Some(order.sales_owner_user_id.clone()),
            business_org_unit_id: Some(order.business_org_unit_id.clone()),
            operator_user_ids: vec![row.stable.created_by.clone()],
            secondary_operator_user_ids: Vec::new(),
            linked_document_id: order.base.id.clone(),
            linked_document_version: order.base.version,
        };
        if !FundsAccess::allows(access, &facts)? || !matches_linked_condition(&facts, condition) {
            continue;
        }
        row.id.hash(fingerprint);
        row.version.hash(fingerprint);
        order.base.id.hash(fingerprint);
        order.base.version.hash(fingerprint);
        filtered.push(row);
    }
    Ok(filtered)
}

/// 当前页复用净核销份额和分录，金额与责任字段沿用原整单口径。
pub(super) fn receivable_page_rows(
    rows: &[ReceivableAccountRow],
    orders: &HashMap<String, SalesOrder>,
    facts: &mut ReceivableFacts,
) -> Vec<ScopedReceivableAccountRow> {
    rows.iter()
        .map(|row| {
            let order = orders.get(&row.sales_order_id);
            ScopedReceivableAccountRow {
                id: row.id.clone(),
                sales_order_id: row.sales_order_id.clone(),
                sales_order_no: order.map(|item| item.order_no.clone()).unwrap_or_default(),
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
                sales_owner_user_id: order.map(|item| item.sales_owner_user_id.clone()),
                business_org_unit_id: order.map(|item| item.business_org_unit_id.clone()),
                entries: facts.take_entries(&row.id),
            }
        })
        .collect()
}

/// 全部已决行按原顺序汇总，当前页取走分录不影响复用的核销净额。
pub(super) fn receivable_summary(
    rows: &[ReceivableAccountRow],
    orders: &HashMap<String, SalesOrder>,
    facts: &ReceivableFacts,
    version: &str,
) -> Result<FundsSummaryView> {
    let mut triples = Vec::with_capacity(rows.len());
    let mut whole_sum = zero_amount();
    for row in rows {
        triples.push((row.id.clone(), facts.share(&row.id), Some(row.sales_order_id.clone())));
        whole_sum = whole_sum.checked_add(row.gross_total);
    }
    let owner_of = orders
        .iter()
        .map(|(id, order)| (id.clone(), order.sales_owner_user_id.clone()))
        .collect::<HashMap<_, _>>();
    build_summary(&triples, &owner_of, Some(whole_sum), version, false)
}
