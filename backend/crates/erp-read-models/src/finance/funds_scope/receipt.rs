//! 客户回款范围查询与金额裁剪。

use std::collections::{BTreeSet, HashMap};

use application_core::AuditActor;
use erp_core::ids::CustomerReceiptId;
use erp_core::money::Amount;
use erp_finance::dto::receivable::CustomerReceiptListQuery;
use erp_finance::entity::read_coverage::whole_document_readable;
use erp_finance::entity::receivable::{AllocationAction, ReceiptAllocation};
use erp_finance::repository::keyword::FinanceSearchTarget;
use erp_finance::repository::prelude::*;
use erp_finance::repository::{CustomerReceiptFilter, ReceivableExt};
use persistence_core::{Executor, Transactional};
use validator::Validate;

use super::authorization::*;
use super::repository::{flow, receipt as receipt_repository};
use super::rows::*;
use crate::finance::search::keyword_ids;
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
#[cfg(test)]
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
        checked_revalidated(
            expected,
            || self.snapshot_customer_receipts(params, query, actor),
            || self.revalidate_customer_receipts(query, actor),
        )
        .await
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

    /// 基础条件只构造明确仓储筛选，关联销售和子账由最终聚合判定。
    async fn receipt_filter(&self, query: &CustomerReceiptListQuery) -> Result<CustomerReceiptFilter> {
        Ok(CustomerReceiptFilter {
            keyword_ids: keyword_ids(&self.db, query.q.as_deref(), FinanceSearchTarget::Receipt).await?,
            receipt_no: query.receipt_no.clone(),
            counterparty_party_id: query.counterparty_party_id.clone(),
            status: query.status,
            ..Default::default()
        })
    }

    /// 最终授权份额和业务条件在数据库形成后分页，页外只装载窄汇总及版本。
    pub(super) async fn load_customer_receipts(
        &self,
        params: &erp_finance::dto::receivable::CustomerReceiptListParams,
        query: &CustomerReceiptListQuery,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<FundsScopedPage<ScopedCustomerReceiptRow>> {
        let (_, authorization) = self.resolve(actor, "customer_receipt", "list", executor).await?;
        if authorization.empty() {
            return Ok(empty_page(&authorization, "no_scope", "回款无可见范围"));
        }
        let condition = self.receipt_condition(query, executor).await?;
        let filter = self.receipt_filter(query).await?;
        let snapshot =
            receipt_repository::page(&self.db, query, &filter, &authorization, &condition, executor).await?;
        let total = snapshot.count()?;
        let version = flow::version(&authorization.context.scope_version, &snapshot.versions);
        ensure_version(params.scope_version.as_deref(), &version).map_err(|_| changed())?;
        let summary = flow::summary(&snapshot.summary, &version)?;
        let items = self
            .receipt_page(snapshot.items, &snapshot.versions, authorization.ledger_read, executor)
            .await?;
        Ok(FundsScopedPage {
            items,
            total,
            summary,
            page: query.paging.page,
            page_size: query.paging.page_size,
            scope_version: version,
            policy_version: authorization.context.policy_version,
            organization_version: authorization.context.organizations.version,
            as_of: authorization.context.as_of.as_utc().to_rfc3339(),
            empty_reason: None,
            scope_summary: "回款继承销售核销来源范围；整单另需财务整账职责及全部来源可见",
            ownership_basis: "linked_sales_owner_and_receipt_operator",
        })
    }

    /// 新事务只重读完整候选责任版本，权限变化拒绝交付第一次页面。
    async fn revalidate_customer_receipts(
        &self,
        query: &CustomerReceiptListQuery,
        actor: &AuditActor,
    ) -> Result<String> {
        let this = self.clone();
        let query = query.clone();
        let actor = actor.clone();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let (_, authorization) =
                        this.resolve(&actor, "customer_receipt", "list", executor).await?;
                    if authorization.empty() {
                        return Ok(authorization.context.scope_version);
                    }
                    let condition = this.receipt_condition(&query, executor).await?;
                    let filter = this.receipt_filter(&query).await?;
                    let versions = receipt_repository::versions(
                        &this.db,
                        &query,
                        &filter,
                        &authorization,
                        &condition,
                        executor,
                    )
                    .await?;
                    if u64::try_from(versions.len()).unwrap_or(u64::MAX) >= flow::FLOW_LIMIT {
                        return Err(Error::ValidationError(
                            "回款查询超过上限，请收窄组织或负责人条件".into(),
                        ));
                    }
                    Ok(flow::version(&authorization.context.scope_version, &versions))
                })
            })
            .await
    }

    /// 当前页复用正式分配裁剪规则，最终授权来源取同拍数据库完整版本分支。
    async fn receipt_page(
        &self,
        rows: Vec<CustomerReceiptRow>,
        versions: &[flow::FlowVersion],
        ledger_read: bool,
        executor: &mut dyn Executor,
    ) -> Result<Vec<ScopedCustomerReceiptRow>> {
        let ids = rows.iter().map(|row| row.id.clone()).collect::<Vec<_>>();
        let links = self.receipt_matched_links(&ids, executor).await?;
        let facts = self
            .sales_fact_map(
                &links.values().flatten().filter_map(|link| link.order.clone()).collect::<Vec<_>>(),
                executor,
            )
            .await?;
        let allowed = Some(
            versions.iter().flat_map(|row| row.sources.iter().map(|source| source.id.clone())).collect(),
        );
        let expected = rows.len();
        let decided = decide_receipt_rows(
            rows,
            &links,
            &facts,
            &allowed,
            &HashMap::new(),
            &FundsLinkedCondition::default(),
            ledger_read,
        );
        if decided.len() != expected {
            return Err(Error::Internal("回款来源聚合与正式裁剪规则不一致".into()));
        }
        let links = self.receipt_decided_links(&ids, &links, executor).await?;
        Ok(self.receipt_page_items(&decided, &links, ledger_read))
    }
}

use erp_finance::repository::CustomerReceiptRow;

impl FundsAccess {
    /// 核销金额保留原已决回款集合的查询边界和返回顺序，来源映射复用同拍事实。
    async fn receipt_decided_links(
        &self,
        ids: &[String],
        candidate_links: &HashMap<String, Vec<ReceiptLink>>,
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, Vec<ReceiptLink>>> {
        let keys = ids.iter().map(CustomerReceiptId::new).collect::<Vec<_>>();
        let allocations = self.db.receipt_allocations().find_allocations_by_receipts(&keys, executor).await?;
        let orders =
            candidate_links.values().flatten().map(|link| (link.id.clone(), link.order.clone())).collect();
        Ok(receipt_links_from_allocations(allocations, &orders))
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
}

/// 核销按本次查询返回流生成金额和展示视图，缺失来源仍保留 None。
fn receipt_links_from_allocations(
    allocations: Vec<ReceiptAllocation>,
    orders: &HashMap<String, LinkedOrderId>,
) -> HashMap<String, Vec<ReceiptLink>> {
    let mut links: HashMap<String, Vec<ReceiptLink>> = HashMap::new();
    for item in allocations {
        let signed = match item.allocation_action {
            AllocationAction::Apply => item.allocated_amount,
            AllocationAction::Reverse => zero_amount().checked_sub(item.allocated_amount),
        };
        let order = orders.get(&item.base.id).cloned().flatten();
        let view = receipt_allocation_view(&item);
        let id = item.base.id;
        let receipt = item.customer_receipt_id.to_string();
        links.entry(receipt).or_default().push(ReceiptLink { id, signed, order, view });
    }
    links
}

/// 按原候选顺序判定行和份额，缺失来源拒绝，未分配仍沿用整账覆盖判定。
fn decide_receipt_rows(
    rows: Vec<CustomerReceiptRow>,
    links: &HashMap<String, Vec<ReceiptLink>>,
    facts: &HashMap<String, LinkedSalesFact>,
    allowed: &Option<BTreeSet<String>>,
    operators: &HashMap<String, Vec<String>>,
    condition: &FundsLinkedCondition,
    ledger_read: bool,
) -> Vec<(CustomerReceiptRow, Vec<String>)> {
    let mut decided = Vec::new();
    for row in rows {
        let row_links = links.get(&row.id).map(Vec::as_slice).unwrap_or_default();
        if !linked_sources_exist(row_links.iter().map(|link| link.order.as_deref()), facts) {
            continue;
        }
        let tuples = receipt_tuples(&row, row_links, facts);
        let matched = matched_orders(&tuples, allowed);
        let matched = filter_matched_orders(
            &tuples,
            matched,
            condition.owner_user_ids.as_deref(),
            condition.org_unit_ids.as_deref(),
        );
        if (condition.owner_user_ids.is_some() || condition.org_unit_ids.is_some()) && matched.is_empty() {
            continue;
        }
        let whole = whole_document_readable(
            ledger_read,
            row_links.iter().map(|link| link.order.as_deref()),
            &matched,
        );
        if !whole && matched.is_empty() {
            continue;
        }
        let doc_operators = operators.get(&row.id).map(Vec::as_slice).unwrap_or_default();
        if !matches_multi_condition(&tuples, doc_operators, &[], condition) {
            continue;
        }
        decided.push((row, matched));
    }
    decided
}

impl FundsAccess {
    /// 单行裁剪；整单金额、登记信息与编辑版本仅整单资格返回。
    ///
    /// # 参数
    /// `row` 为回款事实，`links` 为核销来源，`matched` 为获授权来源；
    /// `whole` 必须由现有整单读取资格判定取得。
    ///
    /// # 返回
    /// 部分授权只保留获授权份额，金额为 null 且不返回整单登记信息。
    ///
    /// # 错误
    /// 本方法不产生业务错误。
    pub(super) fn cut_receipt_row(
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
            counterparty_party_id: whole.then(|| row.counterparty_party_id.clone()),
            customer_id: whole.then(|| row.customer_id.clone()).flatten(),
            bank_reference: whole.then(|| row.bank_reference.clone()).flatten(),
            version: whole.then_some(row.version),
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
        let (_access, authorization) = self.resolve(actor, "customer_receipt", "detail", executor).await?;
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
        let row_links = links.get(&row.id).map(Vec::as_slice).unwrap_or_default();
        if !linked_sources_exist(row_links.iter().map(|link| link.order.as_deref()), &facts) {
            return Err(Error::NotFound("客户回款单不存在".into()));
        }
        let tuples = receipt_tuples(&row, row_links, &facts);
        let matched = matched_orders(&tuples, &allowed);
        let whole = whole_document_readable(
            authorization.ledger_read,
            row_links.iter().map(|link| link.order.as_deref()),
            &matched,
        );
        if !whole && matched.is_empty() {
            return Err(Error::NotFound("客户回款单不存在".into()));
        }
        let data = Self::cut_receipt_row(&row, row_links, &matched, whole);
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
            scope_summary: "回款继承销售核销来源范围；整单另需财务整账职责及全部来源可见",
            ownership_basis: "linked_sales_owner_and_receipt_operator",
        })
    }
}

impl FundsAccess {
    /// 只对当前分页已授权行投影金额，沿用逐单据整账覆盖判定。
    fn receipt_page_items(
        &self,
        rows: &[(CustomerReceiptRow, Vec<String>)],
        links: &HashMap<String, Vec<ReceiptLink>>,
        ledger_read: bool,
    ) -> Vec<ScopedCustomerReceiptRow> {
        rows.iter()
            .map(|(row, matched)| {
                let row_links = links.get(&row.id).map(Vec::as_slice).unwrap_or_default();
                let whole = whole_document_readable(
                    ledger_read,
                    row_links.iter().map(|link| link.order.as_deref()),
                    matched,
                );
                Self::cut_receipt_row(row, row_links, matched, whole)
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use erp_core::common::time::Instant;
    use erp_core::ids::{ReceiptAllocationId, ReceivableEntryId};
    use erp_finance::dto::receivable::ReceiptAllocationView;
    use erp_finance::entity::receivable::{AllocationAction, CustomerReceiptStatus, ReceiptAllocationData};

    use super::*;

    /// 构造已有核销链事实，供真实筛选与金额投影使用。
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

    /// 构造仓储回款候选行。
    fn receipt(id: &str) -> CustomerReceiptRow {
        CustomerReceiptRow {
            id: id.into(),
            status: CustomerReceiptStatus::Posted,
            receipt_no: id.into(),
            counterparty_party_id: "party".into(),
            customer_id: None,
            received_at: Instant::from_unix_secs(1),
            amount: "100".parse().unwrap(),
            bank_reference: None,
            version: 2,
            created_at: 1,
            pending_allocations: Vec::new(),
        }
    }

    /// 整单授权保留恢复草稿所需的原登记信息与提交版本。
    #[test]
    fn receipt_whole_projection_keeps_registration_metadata() {
        let mut row = receipt("draft");
        row.status = CustomerReceiptStatus::Draft;
        row.customer_id = Some("customer".into());
        row.bank_reference = Some("BANK-DRAFT".into());
        let view = FundsAccess::cut_receipt_row(&row, &[], &[], true);
        assert_eq!(view.counterparty_party_id.as_deref(), Some("party"));
        assert_eq!(view.customer_id.as_deref(), Some("customer"));
        assert_eq!(view.bank_reference.as_deref(), Some("BANK-DRAFT"));
        assert_eq!(view.version, Some(2));
        assert_eq!(view.amount, Some(row.amount));
        assert_eq!(view.unallocated_amount, Some(row.amount));
        assert!(!view.permission_limited);
    }

    /// 部分授权不泄露登记信息、编辑版本、整单余额及其他订单的核销份额。
    #[test]
    fn receipt_partial_projection_omits_registration_metadata() {
        let mut row = receipt("receipt");
        row.customer_id = Some("customer".into());
        row.bank_reference = Some("BANK-PRIVATE".into());
        let links = [link("a", Some("so-a"), "60"), link("b", Some("so-b"), "40")];
        let view = FundsAccess::cut_receipt_row(&row, &links, &["so-a".into()], false);
        assert!(view.permission_limited);
        assert_eq!(view.visible_allocated_share, "60".parse().unwrap());
        assert_eq!(view.amount, None);
        assert_eq!(view.allocated_total, None);
        assert_eq!(view.unallocated_amount, None);
        assert_eq!(view.allocations.as_ref().unwrap().len(), 1);
        assert_eq!(view.allocations.as_ref().unwrap()[0].id, "a");
        let json = serde_json::to_value(view).unwrap();
        for field in ["counterparty_party_id", "customer_id", "bank_reference", "version"] {
            assert!(json.get(field).is_none(), "部分授权不应返回 {field}");
        }
    }

    /// 构造不附加业务条件的筛选口径。
    fn empty_condition() -> FundsLinkedCondition {
        FundsLinkedCondition {
            owner_user_ids: None,
            operator_user_ids: None,
            secondary_operator_user_ids: None,
            org_unit_ids: None,
        }
    }

    /// 构造同一事务内的来源责任事实。
    fn sales_fact(owner: &str, org: &str, version: u64) -> LinkedSalesFact {
        LinkedSalesFact { owner_user_id: owner.into(), business_org_unit_id: org.into(), version }
    }

    /// 构造已决回款核销查询返回的真实正向或反向分配。
    fn persisted_allocation(
        id: &str,
        sequence: u32,
        action: AllocationAction,
        amount: &str,
    ) -> ReceiptAllocation {
        ReceiptAllocation::new(
            ReceiptAllocationId::new(id),
            ReceiptAllocationData {
                customer_receipt_id: CustomerReceiptId::new("receipt"),
                receivable_entry_id: ReceivableEntryId::new(format!("entry-{id}")),
                allocation_seq: sequence,
                allocation_action: action,
                allocated_amount: amount.parse().unwrap(),
                allocated_at: Instant::from_unix_secs(10),
                reverses_allocation_id: (action == AllocationAction::Reverse)
                    .then(|| ReceiptAllocationId::new("original")),
            },
        )
        .unwrap()
    }

    /// 金额链接沿用已决集合查询返回顺序，来源映射的键序及多余候选不参与排序。
    #[test]
    fn receipt_decided_links_keep_allocation_stream_order_source_mapping_and_extreme_amount_fold() {
        let max = "79228162514264337593543950335";
        let orders = HashMap::from([
            ("apply-max".into(), Some("so".into())),
            ("apply-one".into(), Some("so".into())),
            ("reverse-one".into(), Some("so".into())),
            ("candidate-only".into(), Some("other".into())),
        ]);
        let found = receipt_links_from_allocations(
            vec![
                persisted_allocation("apply-max", 1, AllocationAction::Apply, max),
                persisted_allocation("reverse-one", 3, AllocationAction::Reverse, "1"),
                persisted_allocation("apply-one", 2, AllocationAction::Apply, "1"),
            ],
            &orders,
        );
        let links = &found["receipt"];
        assert_eq!(
            links.iter().map(|link| link.id.as_str()).collect::<Vec<_>>(),
            ["apply-max", "reverse-one", "apply-one"]
        );
        assert_eq!(links.iter().map(|link| link.view.allocation_seq).collect::<Vec<_>>(), [1, 3, 2]);
        assert_eq!(sum_all(links), max.parse().unwrap());
        assert_eq!(sum_signed(links, &["so".into()]), max.parse().unwrap());
        assert_eq!(links[1].signed, "-1".parse().unwrap());
        assert_eq!(links[1].view.allocated_amount, "1".parse().unwrap());
        assert_eq!(links[1].view.reverses_allocation_id.as_deref(), Some("original"));
    }

    /// 缺失或断裂来源仍为 None，使用回读分配的金额与元数据，不补造来源。
    #[test]
    fn receipt_decided_links_keep_missing_source_and_empty_query_behavior() {
        let orders = HashMap::from([("dangling".into(), None)]);
        let found = receipt_links_from_allocations(
            vec![
                persisted_allocation("missing", 2, AllocationAction::Apply, "5.01"),
                persisted_allocation("dangling", 1, AllocationAction::Reverse, "2.01"),
            ],
            &orders,
        );
        let links = &found["receipt"];
        assert_eq!(links.len(), 2);
        assert!(links.iter().all(|link| link.order.is_none()));
        assert_eq!(sum_all(links), "3".parse().unwrap());
        assert_eq!(sum_signed(links, &[]), Amount::zero());
        assert_eq!(links[0].view.receivable_entry_id, "entry-missing");
        assert_eq!(links[0].view.allocated_at, Instant::from_unix_secs(10));
        assert!(receipt_links_from_allocations(Vec::new(), &orders).is_empty());
    }

    /// 已决行保留候选顺序，金额和版本复用同拍来源事实。
    #[test]
    fn receipt_snapshot_decision_keeps_order_and_uses_same_facts_for_filtered_shares() {
        let links = HashMap::from([
            (
                "second".into(),
                vec![
                    link("a1", Some("so-a"), "60"),
                    link("b1", Some("so-b"), "40"),
                    link("a2", Some("so-a"), "-10"),
                ],
            ),
            ("first".into(), vec![link("a3", Some("so-a"), "7.01")]),
        ]);
        let facts = HashMap::from([
            ("so-a".into(), sales_fact("a", "org-a", 3)),
            ("so-b".into(), sales_fact("b", "org-b", 9)),
            ("irrelevant".into(), sales_fact("c", "org-c", 100)),
        ]);
        let decided = decide_receipt_rows(
            vec![receipt("second"), receipt("first")],
            &links,
            &facts,
            &Some(BTreeSet::from(["so-a".into()])),
            &HashMap::new(),
            &empty_condition(),
            true,
        );
        assert_eq!(decided.iter().map(|(row, _)| row.id.as_str()).collect::<Vec<_>>(), ["second", "first"]);
        assert_eq!(decided[0].1, ["so-a"]);
        assert_eq!(sum_signed(&links["second"], &decided[0].1), "50".parse().unwrap());
        assert_eq!(sum_signed(&links["first"], &decided[1].1), "7.01".parse().unwrap());
        assert_eq!(facts[&decided[0].1[0]].version, 3);
        assert!(!whole_document_readable(
            true,
            links["second"].iter().map(|link| link.order.as_deref()),
            &decided[0].1,
        ));
    }

    /// 关联缺失失败关闭，零分配沿用整账职责边界。
    #[test]
    fn receipt_snapshot_decision_rejects_missing_sources_and_preserves_empty_ledger_boundary() {
        let links = HashMap::from([
            ("missing".into(), vec![link("bad", Some("deleted"), "10")]),
            ("dangling".into(), vec![link("bad2", None, "10")]),
        ]);
        let rows = vec![receipt("missing"), receipt("empty"), receipt("dangling")];
        let decided = decide_receipt_rows(
            rows.clone(),
            &links,
            &HashMap::new(),
            &None,
            &HashMap::new(),
            &empty_condition(),
            true,
        );
        assert_eq!(decided.len(), 1);
        assert_eq!(decided[0].0.id, "empty");
        assert!(decided[0].1.is_empty());
        assert!(
            decide_receipt_rows(
                rows,
                &links,
                &HashMap::new(),
                &None,
                &HashMap::new(),
                &empty_condition(),
                false,
            )
            .is_empty()
        );
    }

    /// 业务筛选条件保持同来源份额和经办人的交集约束。
    #[test]
    fn receipt_snapshot_decision_keeps_owner_org_and_operator_conditions_conjunctive() {
        let links =
            HashMap::from([("r".into(), vec![link("a", Some("so-a"), "60"), link("b", Some("so-b"), "40")])]);
        let facts = HashMap::from([
            ("so-a".into(), sales_fact("a", "org-a", 1)),
            ("so-b".into(), sales_fact("b", "org-b", 2)),
        ]);
        let operators = HashMap::from([("r".into(), vec!["operator".into()])]);
        let mut condition = empty_condition();
        condition.owner_user_ids = Some(vec!["a".into()]);
        condition.org_unit_ids = Some(vec!["org-b".into()]);
        condition.operator_user_ids = Some(vec!["operator".into()]);
        assert!(
            decide_receipt_rows(vec![receipt("r")], &links, &facts, &None, &operators, &condition, true)
                .is_empty()
        );
        condition.org_unit_ids = Some(vec!["org-a".into()]);
        let decided =
            decide_receipt_rows(vec![receipt("r")], &links, &facts, &None, &operators, &condition, true);
        assert_eq!(decided[0].1, ["so-a"]);
        condition.operator_user_ids = Some(vec!["someone_else".into()]);
        assert!(
            decide_receipt_rows(vec![receipt("r")], &links, &facts, &None, &operators, &condition, true)
                .is_empty()
        );
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
