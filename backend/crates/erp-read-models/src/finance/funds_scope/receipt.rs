//! 客户回款范围查询与金额裁剪。

use std::collections::{BTreeSet, HashMap};
use std::hash::{Hash, Hasher};

use application_core::AuditActor;
use erp_core::money::Amount;
use erp_finance::ports::funds_scope::FundsResolvedScope;
use erp_finance::repository::ReceivableExt;
use erp_finance::repository::prelude::*;
use persistence_core::{Executor, Transactional};
use validator::Validate;

use super::authorization::*;
use super::rows::*;
use crate::{Error, Result};

/// 回款分配实体转响应视图；与现有回款投影保持同一字段口径。
pub(super) fn receipt_allocation_view(
    item: &erp_finance::entity::receivable::ReceiptAllocation,
) -> erp_finance::dto::receivable::ReceiptAllocationView {
    erp_finance::dto::receivable::ReceiptAllocationView {
        id: item.base.id.clone(),
        allocation_seq: item.allocation_seq,
        allocation_action: item.allocation_action,
        receivable_entry_id: item.receivable_entry_id.to_string(),
        allocated_amount: item.allocated_amount,
        allocated_at: item.allocated_at,
        reverses_allocation_id: item.reverses_allocation_id.as_ref().map(|id| id.to_string()),
    }
}
/// 回款行关联元组；缺失订单的分配保留未分配归属，不丢份额。
pub(super) fn receipt_tuples(
    row: &CustomerReceiptRow,
    links: &[ReceiptLink],
    facts: &HashMap<String, LinkedSalesFact>,
) -> Vec<OrderTuple> {
    let mut seen = BTreeSet::new();
    let mut tuples = Vec::new();
    for link in links {
        let key = link.order.clone().unwrap_or_default();
        if !seen.insert(key) {
            continue;
        }
        match link.order.as_ref().and_then(|id| facts.get(id)) {
            Some(fact) => tuples.push((
                link.order.clone(),
                Some(fact.owner_user_id.clone()),
                Some(fact.business_org_unit_id.clone()),
                row.id.clone(),
                row.version,
            )),
            None => tuples.push((None, None, None, row.id.clone(), row.version)),
        }
    }
    tuples
}

/// 授权集合内的归属订单；`None` 表示公司范围不限制关联单。
pub(super) fn matched_orders(tuples: &[OrderTuple], allowed: &Option<BTreeSet<String>>) -> Vec<String> {
    tuples
        .iter()
        .filter_map(|(order, _, _, _, _)| order.clone())
        .filter(|order| allowed.as_ref().is_none_or(|set| set.contains(order)))
        .collect::<Vec<_>>()
        .into_iter()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

/// 归属筛选逐份额收窄，不能仅以单据内另一条关联命中作为份额依据。
pub(super) fn filter_matched_orders(
    tuples: &[OrderTuple],
    matched: Vec<String>,
    owners: Option<&[String]>,
    orgs: Option<&[String]>,
) -> Vec<String> {
    matched
        .into_iter()
        .filter(|id| {
            tuples.iter().any(|(order, owner, org, _, _)| {
                order.as_ref() == Some(id)
                    && owners.is_none_or(|ids| owner.as_ref().is_some_and(|value| ids.contains(value)))
                    && orgs.is_none_or(|ids| org.as_ref().is_some_and(|value| ids.contains(value)))
            })
        })
        .collect()
}

/// 整单口径求和；仅整单读取资格持有者可见的结果使用，不得用于部分授权。
pub(super) fn sum_all(links: &[ReceiptLink]) -> Amount {
    let mut total = zero_amount();
    for link in links {
        total = total.checked_add(link.signed);
    }
    total
}

/// 汇总输入：匹配份额与未分配份额进入汇总，未授权订单份额不得进入。
pub(super) fn summary_inputs(
    links: &[ReceiptLink],
    matched: &[String],
) -> Vec<(String, Amount, LinkedOrderId)> {
    links
        .iter()
        .filter(|link| link.order.as_ref().is_none_or(|order| matched.iter().any(|id| id == order)))
        .map(|link| (link.id.clone(), link.signed, link.order.clone()))
        .collect()
}

/// 匹配份额求和；方向已在装载时按正反动作记入金额符号，不做差额推导。
pub(super) fn sum_signed(links: &[ReceiptLink], matched: &[String]) -> Amount {
    let mut total = zero_amount();
    for link in links {
        if !link.order.as_ref().is_some_and(|order| matched.iter().any(|id| id == order)) {
            continue;
        }
        total = total.checked_add(link.signed);
    }
    total
}

impl FundsAccess {
    /// 分页查询客户回款范围行：负责销售与登记/核销经办人分别查询。
    pub async fn customer_receipt_list_scoped(
        &self,
        params: &erp_finance::dto::receivable::CustomerReceiptListParams,
        actor: &AuditActor,
    ) -> Result<FundsScopedPage<ScopedCustomerReceiptRow>> {
        params.validate()?;
        let query = params.normalized().map_err(Error::from)?;
        ensure_page(query.paging.page, params.scope_version.as_deref())?;
        let snapshot =
            self.checked_customer_receipts(params, &query, actor, params.scope_version.as_deref()).await?;
        Ok(snapshot)
    }

    /// 独立详情重新解析回款详情动作；不可见与不存在统一为 NotFound。
    pub async fn customer_receipt_detail_scoped(
        &self,
        id: &str,
        actor: &AuditActor,
    ) -> Result<FundsScopedResult<ScopedCustomerReceiptRow>> {
        let this = self.clone();
        let actor = actor.clone();
        let id = id.to_string();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move { this.load_customer_receipt_detail(&id, &actor, executor).await })
            })
            .await
    }

    /// 返回前重读授权及候选事实版本；变化时拒绝交付原结果。
    pub(super) async fn checked_customer_receipts(
        &self,
        params: &erp_finance::dto::receivable::CustomerReceiptListParams,
        query: &erp_finance::dto::receivable::CustomerReceiptListQuery,
        actor: &AuditActor,
        expected: Option<&str>,
    ) -> Result<FundsScopedPage<ScopedCustomerReceiptRow>> {
        checked_twice(expected, || self.snapshot_customer_receipts(params, query, actor)).await
    }

    /// 身份和业务事实均使用调用方同一事务，不缓存权限解析结果。
    pub(super) async fn snapshot_customer_receipts(
        &self,
        params: &erp_finance::dto::receivable::CustomerReceiptListParams,
        query: &erp_finance::dto::receivable::CustomerReceiptListQuery,
        actor: &AuditActor,
    ) -> Result<FundsScopedPage<ScopedCustomerReceiptRow>> {
        let this = self.clone();
        let params = params.clone();
        let query = query.clone();
        let actor = actor.clone();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move { this.load_customer_receipts(&params, &query, &actor, executor).await })
            })
            .await
    }

    /// 业务筛选复用现有仓储查询，授权过滤与金额裁剪在同一事务内完成。
    pub(super) async fn load_customer_receipts(
        &self,
        params: &erp_finance::dto::receivable::CustomerReceiptListParams,
        query: &erp_finance::dto::receivable::CustomerReceiptListQuery,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<FundsScopedPage<ScopedCustomerReceiptRow>> {
        use erp_finance::dto::receivable::SortDir;
        let (access, authorization) = self.resolve(actor, "customer_receipt", "list", executor).await?;
        if authorization.empty() {
            return Ok(empty_page(&authorization, "no_scope", "回款无可见范围"));
        }
        let keyword_ids = crate::finance::search::keyword_ids(
            &self.db,
            query.q.as_deref(),
            erp_finance::repository::keyword::FinanceSearchTarget::Receipt,
        )
        .await?;
        let scope_query = erp_finance::repository::ScopedCustomerReceiptQuery {
            keyword_ids,
            receipt_no: query.receipt_no.clone(),
            counterparty_party_id: query.counterparty_party_id.clone(),
            status: query.status,
            scope: erp_finance::repository::ReceivableListScope {
                sales_order_id: query.sales_order_id.clone(),
                receivable_account_id: query.receivable_account_id.clone(),
            },
            page: 1,
            page_size: 10_000,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: matches!(query.paging.sort_dir, SortDir::Asc),
        };
        let candidates =
            self.db.receivable().search_customer_receipts_in_account_scope(&scope_query, executor).await?;
        if candidates.items.len() >= 10_000 {
            return Err(Error::ValidationError("回款查询超过上限，请收窄组织或负责人条件".into()));
        }
        let decided = self
            .assemble_customer_receipts(query, candidates.items, access, &authorization, executor)
            .await?;
        let ids = decided.iter().map(|(row, _)| row.id.clone()).collect::<Vec<_>>();
        let links = self.receipt_matched_links(&ids, executor).await?;
        let order_ids = links.values().flatten().filter_map(|link| link.order.clone()).collect::<Vec<_>>();
        let facts = self.sales_fact_map(&order_ids, executor).await?;
        let mut fingerprint = std::collections::hash_map::DefaultHasher::new();
        authorization.context.scope_version.hash(&mut fingerprint);
        for (row, matched) in decided.iter() {
            row.id.hash(&mut fingerprint);
            row.version.hash(&mut fingerprint);
            for order in matched.iter().filter_map(|id| facts.get(id)) {
                order.version.hash(&mut fingerprint);
            }
        }
        self.finish_customer_receipts(
            params,
            query,
            decided,
            links,
            facts,
            authorization,
            fingerprint,
            executor,
        )
        .await
    }
}

use erp_finance::repository::CustomerReceiptRow;

impl FundsAccess {
    /// 回款经办人事实：登记取创建审计，核销取审批提交人；无过滤条件时不读审计。
    pub(super) async fn receipt_operators(
        &self,
        ids: &[String],
        kind: Option<erp_finance::dto::receivable::ReceiptOperatorKind>,
        want: bool,
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, Vec<String>>> {
        use erp_finance::dto::receivable::ReceiptOperatorKind;
        if !want {
            return Ok(HashMap::new());
        }
        match kind {
            Some(ReceiptOperatorKind::Register) => {
                self.audit_operators(
                    "customer_receipt",
                    ids,
                    |action| action == "customer_receipt.create",
                    executor,
                )
                .await
            },
            Some(ReceiptOperatorKind::Settle) => self.receipt_settle_operators(ids, executor).await,
            None => Ok(HashMap::new()),
        }
    }

    /// 回款关联筛选条件：负责人、经办人与组织分别精确匹配，同字段 OR、异字段 AND。
    pub(super) async fn receipt_condition(
        &self,
        query: &erp_finance::dto::receivable::CustomerReceiptListQuery,
        executor: &mut dyn Executor,
    ) -> Result<FundsLinkedCondition> {
        let org_unit_ids = match &query.org_unit_ids {
            Some(ids) => {
                let list = ids.as_slice().to_vec();
                let expanded = self
                    .expand_org_units(&list, query.include_descendants.unwrap_or(false), executor)
                    .await?;
                Some(expanded.into_iter().collect::<Vec<_>>())
            },
            None => None,
        };
        Ok(FundsLinkedCondition {
            owner_user_ids: query.sales_owner_user_ids.as_ref().map(|ids| ids.as_slice().to_vec()),
            operator_user_ids: query.operator_user_ids.as_ref().map(|ids| ids.as_slice().to_vec()),
            secondary_operator_user_ids: None,
            org_unit_ids,
        })
    }

    /// 回款候选逐行判定可见性与筛选；授权集合外的关联份额不进入行与汇总。
    pub(super) async fn assemble_customer_receipts(
        &self,
        query: &erp_finance::dto::receivable::CustomerReceiptListQuery,
        rows: Vec<CustomerReceiptRow>,
        access: FundsResolvedScope,
        authorization: &FundsAuthorization,
        executor: &mut dyn Executor,
    ) -> Result<Vec<(CustomerReceiptRow, Vec<String>)>> {
        let ids = rows.iter().map(|row| row.id.clone()).collect::<Vec<_>>();
        let links = self.receipt_matched_links(&ids, executor).await?;
        let order_ids = links.values().flatten().filter_map(|link| link.order.clone()).collect::<Vec<_>>();
        let facts = self.sales_fact_map(&order_ids, executor).await?;
        let authorized = self.authorized_sales_ids(authorization, executor).await?;
        let allowed = authorized.map(|list| list.into_iter().collect::<BTreeSet<_>>());
        let operators = self
            .receipt_operators(&ids, query.operator_kind, query.operator_user_ids.is_some(), executor)
            .await?;
        let condition = self.receipt_condition(query, executor).await?;
        let whole = authorization.whole();
        let mut decided = Vec::new();
        for row in rows {
            let empty_links: Vec<ReceiptLink> = Vec::new();
            let row_links = links.get(&row.id).unwrap_or(&empty_links);
            let tuples = receipt_tuples(&row, row_links, &facts);
            let matched = matched_orders(&tuples, &allowed);
            let matched = filter_matched_orders(
                &tuples,
                matched,
                condition.owner_user_ids.as_deref(),
                condition.org_unit_ids.as_deref(),
            );
            if (condition.owner_user_ids.is_some() || condition.org_unit_ids.is_some()) && matched.is_empty()
            {
                continue;
            }
            let visible = row_visible(&access, &tuples, &[], &[])?;
            let unlinked = row_links.iter().all(|link| link.order.is_none());
            if !keep_row(visible, whole, !matched.is_empty(), row_links.is_empty() || unlinked) {
                continue;
            }
            let doc_operators = operators.get(&row.id).cloned().unwrap_or_default();
            if !matches_multi_condition(&tuples, &doc_operators, &[], &condition) {
                continue;
            }
            decided.push((row, matched));
        }
        Ok(decided)
    }
}

impl FundsAccess {
    /// 回款候选分页裁剪与汇总装配；明细、汇总与导出复用同一已决集合。
    // 查询+分页+执行器参数为既有签名，保持调用方一致不拆。
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn finish_customer_receipts(
        &self,
        params: &erp_finance::dto::receivable::CustomerReceiptListParams,
        query: &erp_finance::dto::receivable::CustomerReceiptListQuery,
        decided: Vec<(CustomerReceiptRow, Vec<String>)>,
        links: HashMap<String, Vec<ReceiptLink>>,
        facts: HashMap<String, LinkedSalesFact>,
        authorization: FundsAuthorization,
        fingerprint: std::collections::hash_map::DefaultHasher,
        executor: &mut dyn Executor,
    ) -> Result<FundsScopedPage<ScopedCustomerReceiptRow>> {
        let total = decided.len() as u64;
        let page = query.paging.page.max(1);
        let page_size = query.paging.page_size.max(1);
        let start = ((page - 1) as usize).saturating_mul(page_size as usize);
        let end = start.saturating_add(page_size as usize).min(decided.len());
        let whole = authorization.whole();
        let empty_links: Vec<ReceiptLink> = Vec::new();
        let mut items = Vec::new();
        if start < decided.len() {
            for (row, matched) in decided[start..end].iter() {
                let row_links = links.get(&row.id).unwrap_or(&empty_links);
                items.push(self.cut_receipt_row(row, row_links, matched, whole));
            }
        }
        let mut triples = Vec::new();
        let mut whole_sum = zero_amount();
        for (row, matched) in decided.iter() {
            let row_links = links.get(&row.id).unwrap_or(&empty_links);
            triples.extend(summary_inputs(row_links, matched));
            whole_sum = whole_sum.checked_add(row.amount);
        }
        let owner_of = facts
            .iter()
            .map(|(id, fact)| (id.clone(), fact.owner_user_id.clone()))
            .collect::<HashMap<_, _>>();
        let version = format!("{:x}", fingerprint.finish());
        ensure_version(params.scope_version.as_deref(), &version).map_err(|_| changed())?;
        let summary = build_summary(&triples, &owner_of, whole_amount(whole, whole_sum), &version, !whole)?;
        let owner_options = self.owner_options_sales(&authorization, executor).await?;
        Ok(FundsScopedPage {
            items,
            total,
            summary,
            owner_options,
            page,
            page_size,
            scope_version: version.clone(),
            policy_version: authorization.context.policy_version,
            organization_version: authorization.context.organizations.version,
            as_of: authorization.context.as_of.as_utc().to_rfc3339(),
            empty_reason: None,
            scope_summary: "回款按核销关联销售当前负责人与登记/核销经办人授权；部分授权仅返获授权份额",
            ownership_basis: "linked_sales_owner_and_receipt_operator",
        })
    }

    /// 单行金额裁剪；整单金额与完整分配仅整单资格返回，否则为 null。
    pub(super) fn cut_receipt_row(
        &self,
        row: &CustomerReceiptRow,
        links: &[ReceiptLink],
        matched: &[String],
        whole: bool,
    ) -> ScopedCustomerReceiptRow {
        let mut views: Vec<_> = if whole {
            links.iter().map(|link| link.view.clone()).collect()
        } else {
            links
                .iter()
                .filter(|link| link.order.as_ref().is_some_and(|order| matched.iter().any(|id| id == order)))
                .map(|link| link.view.clone())
                .collect()
        };
        views.sort_by_key(|view| view.allocation_seq);
        let net = sum_all(links);
        ScopedCustomerReceiptRow {
            id: row.id.clone(),
            receipt_no: row.receipt_no.clone(),
            status: row.status,
            received_at: row.received_at,
            created_at: row.created_at,
            visible_allocated_share: sum_signed(links, matched),
            amount: whole_amount(whole, row.amount),
            allocated_total: whole_amount(whole, net),
            unallocated_amount: whole_amount(whole, row.amount.checked_sub(net)),
            allocations: Some(views),
            permission_limited: !whole,
        }
    }

    /// 回款详情同一事务内解析、取数与裁剪；版本绑定关联单据。
    pub(super) async fn load_customer_receipt_detail(
        &self,
        id: &str,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<FundsScopedResult<ScopedCustomerReceiptRow>> {
        use erp_finance::repository::CustomerReceiptFilter;
        let (access, authorization) = self.resolve(actor, "customer_receipt", "detail", executor).await?;
        let filter = CustomerReceiptFilter {
            keyword_ids: None,
            receipt_ids: Some(vec![id.to_string()]),
            pending_entry_ids: Vec::new(),
            receipt_no: None,
            counterparty_party_id: None,
            status: None,
            page: 1,
            page_size: 1,
            sort_by: None,
            sort_ascending: false,
        };
        let page = self.db.customer_receipts().search_customer_receipts(&filter, executor).await?;
        let row = page.items.into_iter().next().ok_or_else(|| Error::NotFound("客户回款单不存在".into()))?;
        let links = self.receipt_matched_links(std::slice::from_ref(&row.id), executor).await?;
        let order_ids = links.values().flatten().filter_map(|link| link.order.clone()).collect::<Vec<_>>();
        let facts = self.sales_fact_map(&order_ids, executor).await?;
        let authorized = self.authorized_sales_ids(&authorization, executor).await?;
        let allowed = authorized.map(|list| list.into_iter().collect::<BTreeSet<_>>());
        let empty_links: Vec<ReceiptLink> = Vec::new();
        let row_links = links.get(&row.id).unwrap_or(&empty_links);
        let tuples = receipt_tuples(&row, row_links, &facts);
        let matched = matched_orders(&tuples, &allowed);
        let whole = authorization.whole();
        let visible = row_visible(&access, &tuples, &[], &[])?;
        let unlinked = row_links.iter().all(|link| link.order.is_none());
        if !keep_row(visible, whole, !matched.is_empty(), row_links.is_empty() || unlinked) {
            return Err(Error::NotFound("客户回款单不存在".into()));
        }
        let data = self.cut_receipt_row(&row, row_links, &matched, whole);
        let mut parts = vec![format!("{}:{}", row.id, row.version)];
        for order in matched.iter().filter_map(|id| facts.get(id)) {
            parts.push(format!("{}:{}", order.owner_user_id, order.version));
        }
        Ok(FundsScopedResult {
            data,
            scope_version: scope_version(&authorization.context, &parts),
            policy_version: authorization.context.policy_version,
            organization_version: authorization.context.organizations.version,
            as_of: authorization.context.as_of.as_utc().to_rfc3339(),
            empty_reason: None,
            scope_summary: "回款按核销关联销售当前负责人与登记/核销经办人授权；部分授权仅返获授权份额",
            ownership_basis: "linked_sales_owner_and_receipt_operator",
        })
    }
}

#[cfg(test)]
mod tests {
    use erp_core::common::time::Instant;
    use erp_finance::dto::receivable::ReceiptAllocationView;
    use erp_finance::entity::receivable::AllocationAction;

    use super::*;

    fn link(id: &str, order: Option<&str>, signed: &str) -> ReceiptLink {
        let amount: Amount = signed.parse().unwrap();
        ReceiptLink {
            id: id.into(),
            order: order.map(str::to_string),
            signed: amount,
            view: ReceiptAllocationView {
                id: id.into(),
                allocation_seq: 1,
                allocation_action: AllocationAction::Apply,
                receivable_entry_id: id.into(),
                allocated_amount: amount,
                allocated_at: Instant::from_unix_secs(1),
                reverses_allocation_id: None,
            },
        }
    }

    #[test]
    fn owner_and_org_filters_reduce_actual_shares_and_summary_with_reversals() {
        let tuples = vec![
            (Some("so-a".into()), Some("a".into()), Some("org-a".into()), "r".into(), 1),
            (Some("so-b".into()), Some("b".into()), Some("org-b".into()), "r".into(), 1),
        ];
        let links = vec![
            link("1", Some("so-a"), "60"),
            link("2", Some("so-b"), "40"),
            link("3", Some("so-a"), "-10"),
            link("4", None, "5"),
        ];
        let owners = HashMap::from([("so-a".into(), "a".into()), ("so-b".into(), "b".into())]);
        for (people, orgs) in [(Some(vec!["a".into()]), None), (None, Some(vec!["org-a".into()]))] {
            let matched = filter_matched_orders(
                &tuples,
                matched_orders(&tuples, &None),
                people.as_deref(),
                orgs.as_deref(),
            );
            assert_eq!(matched, vec!["so-a"]);
            assert_eq!(sum_signed(&links, &matched), "50".parse().unwrap());
            let summary = build_summary(&summary_inputs(&links, &matched), &owners, None, "v", true).unwrap();
            assert_eq!(summary.grouped.len(), 1);
            assert_eq!(summary.grouped[0].visible_share, "50".parse().unwrap());
            assert_eq!(summary.unassigned, zero_amount());
            let whole = build_summary(
                &summary_inputs(&links, &matched),
                &owners,
                Some("95".parse().unwrap()),
                "v",
                false,
            )
            .unwrap();
            assert_eq!(whole.unassigned, "5".parse().unwrap());
        }
        // 不允许负责人命中 A、组织命中 B 后把整单作为同时匹配。
        assert!(
            filter_matched_orders(
                &tuples,
                matched_orders(&tuples, &None),
                Some(&["a".into()]),
                Some(&["org-b".into()])
            )
            .is_empty()
        );
        assert!(!keep_row(true, false, false, true));
        assert!(keep_row(true, true, false, true));
    }
}
