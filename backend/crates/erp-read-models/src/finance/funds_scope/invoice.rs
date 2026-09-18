//! 销项发票范围查询与金额裁剪。

use std::collections::{BTreeSet, HashMap};
use std::hash::{Hash, Hasher};

use application_core::{AuditActor, FilterOption};
use erp_core::money::Amount;
use erp_finance::ports::funds_scope::FundsResolvedScope;
use erp_finance::repository::ReceivableExt;
use erp_finance::repository::prelude::*;
use erp_procurement::PurchaseAccess;
use persistence_core::{Executor, Transactional};
use validator::Validate;

use super::allocation::*;
use super::authorization::*;
use super::receipt::matched_orders;
use super::rows::*;
use crate::{Error, Result};

/// 销项发票分配实体转响应视图；与发票查询服务保持同一字段口径。
pub(super) fn sales_invoice_allocation_view(
    item: &erp_finance::entity::receivable::SalesInvoiceAllocation,
) -> erp_finance::dto::receivable::SalesInvoiceAllocationView {
    erp_finance::dto::receivable::SalesInvoiceAllocationView {
        id: item.base.id.clone(),
        allocation_seq: item.allocation_seq,
        allocation_action: item.allocation_action,
        receivable_account_id: item.receivable_account_id.to_string(),
        allocated_gross_amount: item.allocated_gross_amount,
        allocated_net_amount: item.allocated_net_amount,
        allocated_tax_amount: item.allocated_tax_amount,
        reverses_allocation_id: item.reverses_allocation_id.as_ref().map(|id| id.to_string()),
    }
}
use erp_finance::repository::InvoiceRow;

impl FundsAccess {
    /// 分页查询发票范围行：销项按负责销售，登记经办人与组织分别查询。
    pub async fn invoice_list_scoped(
        &self,
        params: &erp_finance::dto::receivable::InvoiceListParams,
        actor: &AuditActor,
        purchase_access: &PurchaseAccess,
    ) -> Result<FundsScopedPage<ScopedInvoiceRow>> {
        params.validate()?;
        let query = params.normalized().map_err(Error::from)?;
        ensure_page(query.paging.page, params.scope_version.as_deref())?;
        let snapshot = self
            .checked_invoices(params, &query, actor, purchase_access, params.scope_version.as_deref())
            .await?;
        Ok(snapshot)
    }

    /// 独立详情重新解析发票详情动作；不可见与不存在统一为 NotFound。
    pub async fn invoice_detail_scoped(
        &self,
        id: &str,
        actor: &AuditActor,
        purchase_access: &PurchaseAccess,
    ) -> Result<FundsScopedResult<ScopedInvoiceRow>> {
        let this = self.clone();
        let actor = actor.clone();
        let id = id.to_string();
        let purchase_access = purchase_access.clone();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(
                    async move { this.load_invoice_detail(&id, &actor, &purchase_access, executor).await },
                )
            })
            .await
    }

    /// 返回前重读授权及候选事实版本；变化时拒绝交付原结果。
    pub(super) async fn checked_invoices(
        &self,
        params: &erp_finance::dto::receivable::InvoiceListParams,
        query: &erp_finance::dto::receivable::InvoiceListQuery,
        actor: &AuditActor,
        purchase_access: &PurchaseAccess,
        expected: Option<&str>,
    ) -> Result<FundsScopedPage<ScopedInvoiceRow>> {
        let snapshot = self.snapshot_invoices(params, query, actor, purchase_access).await?;
        if expected.is_some_and(|value| value != snapshot.scope_version) {
            return Err(changed());
        }
        let current = self.snapshot_invoices(params, query, actor, purchase_access).await?;
        if current.scope_version != snapshot.scope_version {
            return Err(changed());
        }
        Ok(snapshot)
    }

    /// 身份和业务事实均使用调用方同一事务，不缓存权限解析结果。
    pub(super) async fn snapshot_invoices(
        &self,
        params: &erp_finance::dto::receivable::InvoiceListParams,
        query: &erp_finance::dto::receivable::InvoiceListQuery,
        actor: &AuditActor,
        purchase_access: &PurchaseAccess,
    ) -> Result<FundsScopedPage<ScopedInvoiceRow>> {
        let this = self.clone();
        let params = params.clone();
        let query = query.clone();
        let actor = actor.clone();
        let purchase_access = purchase_access.clone();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    this.load_invoices(&params, &query, &actor, &purchase_access, executor).await
                })
            })
            .await
    }
}

impl FundsAccess {
    /// 分页查询发票范围行：销项按负责销售与登记经办，进项按采购负责人与登记经办。
    pub(super) async fn load_invoices(
        &self,
        params: &erp_finance::dto::receivable::InvoiceListParams,
        query: &erp_finance::dto::receivable::InvoiceListQuery,
        actor: &AuditActor,
        purchase_access: &PurchaseAccess,
        executor: &mut dyn Executor,
    ) -> Result<FundsScopedPage<ScopedInvoiceRow>> {
        use erp_finance::dto::receivable::SortDir;
        use erp_finance::entity::receivable::InvoiceDirection;
        let (access, authorization) =
            self.resolve_dual(actor, "invoice", "list", purchase_access, executor).await?;
        if authorization.empty() {
            return Ok(empty_page(&authorization, "no_scope", "发票无可见范围"));
        }
        let target = match query.invoice_direction {
            Some(InvoiceDirection::Purchase) => {
                erp_finance::repository::keyword::FinanceSearchTarget::PurchaseInvoice
            },
            _ => erp_finance::repository::keyword::FinanceSearchTarget::SalesInvoice,
        };
        let keyword_ids = crate::finance::search::keyword_ids(&self.db, query.q.as_deref(), target).await?;
        let filter = erp_finance::repository::InvoiceFilter {
            keyword_ids,
            invoice_ids: None,
            invoice_direction: query.invoice_direction,
            invoice_kind: query.invoice_kind,
            party_id: query.party_id.clone(),
            invoice_no: query.invoice_no.clone(),
            status: query.status,
            page: 1,
            page_size: 10_000,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: matches!(query.paging.sort_dir, SortDir::Asc),
        };
        let candidates = self.db.invoices().search_invoices(&filter, executor).await?;
        if candidates.items.len() >= 10_000 {
            return Err(Error::ValidationError("发票查询超过上限，请收窄组织或负责人条件".into()));
        }
        let decided = self
            .assemble_invoices(query, candidates.items, &access, &authorization, purchase_access, executor)
            .await?;
        let ids: Vec<String> = decided.iter().map(|(row, _, _)| row.id.clone()).collect();
        let sales_links = self.sales_invoice_matched_links(&ids, executor).await?;
        let purchase_links = self.purchase_invoice_matched_links(&ids, executor).await?;
        let (sales_facts, purchase_facts) =
            self.invoice_fact_maps(&sales_links, &purchase_links, executor).await?;
        let mut fingerprint = std::collections::hash_map::DefaultHasher::new();
        authorization.context.scope_version.hash(&mut fingerprint);
        for (row, sales_matched, purchase_matched) in decided.iter() {
            row.id.hash(&mut fingerprint);
            row.version.hash(&mut fingerprint);
            for order in sales_matched.iter().filter_map(|id| sales_facts.get(id)) {
                order.version.hash(&mut fingerprint);
            }
            for order in purchase_matched.iter().filter_map(|id| purchase_facts.get(id)) {
                order.version.hash(&mut fingerprint);
            }
        }
        self.finish_invoices(
            params,
            query,
            decided,
            sales_links,
            purchase_links,
            sales_facts,
            purchase_facts,
            authorization,
            fingerprint,
            executor,
        )
        .await
    }

    /// 发票双方向关联事实一次取回；缺失订单保留未分配归属。
    pub(super) async fn invoice_fact_maps(
        &self,
        sales_links: &HashMap<String, Vec<SalesInvoiceLink>>,
        purchase_links: &HashMap<String, Vec<PurchaseInvoiceLink>>,
        executor: &mut dyn Executor,
    ) -> Result<(HashMap<String, LinkedSalesFact>, HashMap<String, LinkedPurchaseFact>)> {
        let sales_ids =
            sales_links.values().flatten().filter_map(|link| link.order.clone()).collect::<Vec<_>>();
        let purchase_ids =
            purchase_links.values().flatten().filter_map(|link| link.order.clone()).collect::<Vec<_>>();
        let sales = self.sales_fact_map(&sales_ids, executor).await?;
        let purchase = self.purchase_fact_map(&purchase_ids, executor).await?;
        Ok((sales, purchase))
    }

    /// 发票候选逐行判定可见性与筛选；授权集合外的关联份额不进入行与汇总。
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn assemble_invoices(
        &self,
        query: &erp_finance::dto::receivable::InvoiceListQuery,
        rows: Vec<InvoiceRow>,
        access: &FundsResolvedScope,
        authorization: &FundsAuthorization,
        purchase_access: &PurchaseAccess,
        executor: &mut dyn Executor,
    ) -> Result<Vec<(InvoiceRow, Vec<String>, Vec<String>)>> {
        use erp_finance::entity::receivable::InvoiceDirection;
        let ids = rows.iter().map(|row| row.id.clone()).collect::<Vec<_>>();
        let sales_links = self.sales_invoice_matched_links(&ids, executor).await?;
        let purchase_links = self.purchase_invoice_matched_links(&ids, executor).await?;
        let (sales_facts, purchase_facts) =
            self.invoice_fact_maps(&sales_links, &purchase_links, executor).await?;
        let sales_allowed = self
            .authorized_sales_ids(authorization, executor)
            .await?
            .map(|list| list.into_iter().collect::<BTreeSet<_>>());
        let purchase_scope = authorization.purchase_scope.clone().unwrap_or_default();
        let purchase_allowed = self
            .authorized_purchase_ids(purchase_access, &purchase_scope, executor)
            .await?
            .map(|list| list.into_iter().collect::<BTreeSet<_>>());
        let condition = self.invoice_condition(query, executor).await?;
        let empty_sales: Vec<SalesInvoiceLink> = Vec::new();
        let empty_purchase: Vec<PurchaseInvoiceLink> = Vec::new();
        let mut decided = Vec::new();
        for row in rows {
            let sales = sales_links.get(&row.id).unwrap_or(&empty_sales);
            let purchase = purchase_links.get(&row.id).unwrap_or(&empty_purchase);
            let sales_tuples = invoice_sales_tuples(&row, sales, &sales_facts);
            let purchase_tuples = invoice_purchase_tuples(&row, purchase, &purchase_facts);
            let sales_matched = matched_orders(&sales_tuples, &sales_allowed);
            let purchase_matched = matched_orders(&purchase_tuples, &purchase_allowed);
            let whole = match row.invoice_direction {
                InvoiceDirection::Sales => authorization.whole(),
                InvoiceDirection::Purchase => purchase_whole(authorization),
            };
            let mut combined = sales_tuples.clone();
            combined.extend(purchase_tuples.clone());
            let operators = vec![row.stable.created_by.clone()];
            let visible = row_visible(access, &combined, &operators, &[])?;
            let unlinked = combined.iter().all(|(order, _, _, _, _)| order.is_none());
            let matched_any = !sales_matched.is_empty() || !purchase_matched.is_empty();
            if !keep_row(visible, whole, matched_any, sales.is_empty() && purchase.is_empty() || unlinked) {
                continue;
            }
            if !matches_invoice_condition(&sales_tuples, &purchase_tuples, &operators, &condition) {
                continue;
            }
            if !matches_invoice_business(query, &row, sales, &sales_matched) {
                continue;
            }
            decided.push((row, sales_matched, purchase_matched));
        }
        Ok(decided)
    }

    /// 发票关联筛选条件：销售负责人、采购负责人、登记经办人与组织分别精确匹配。
    pub(super) async fn invoice_condition(
        &self,
        query: &erp_finance::dto::receivable::InvoiceListQuery,
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
            secondary_operator_user_ids: query
                .procurement_owner_user_ids
                .as_ref()
                .map(|ids| ids.as_slice().to_vec()),
            org_unit_ids,
        })
    }

    /// 发票候选分页裁剪与汇总装配；明细、汇总与导出复用同一已决集合。
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn finish_invoices(
        &self,
        params: &erp_finance::dto::receivable::InvoiceListParams,
        query: &erp_finance::dto::receivable::InvoiceListQuery,
        decided: Vec<(InvoiceRow, Vec<String>, Vec<String>)>,
        sales_links: HashMap<String, Vec<SalesInvoiceLink>>,
        purchase_links: HashMap<String, Vec<PurchaseInvoiceLink>>,
        sales_facts: HashMap<String, LinkedSalesFact>,
        purchase_facts: HashMap<String, LinkedPurchaseFact>,
        authorization: FundsAuthorization,
        fingerprint: std::collections::hash_map::DefaultHasher,
        executor: &mut dyn Executor,
    ) -> Result<FundsScopedPage<ScopedInvoiceRow>> {
        use erp_finance::entity::receivable::InvoiceDirection;
        let total = decided.len() as u64;
        let page = query.paging.page.max(1);
        let page_size = query.paging.page_size.max(1);
        let start = ((page - 1) as usize).saturating_mul(page_size as usize);
        let end = start.saturating_add(page_size as usize).min(decided.len());
        let empty_sales: Vec<SalesInvoiceLink> = Vec::new();
        let empty_purchase: Vec<PurchaseInvoiceLink> = Vec::new();
        let mut items = Vec::new();
        if start < decided.len() {
            for (row, sales_matched, purchase_matched) in decided[start..end].iter() {
                let sales = sales_links.get(&row.id).unwrap_or(&empty_sales);
                let purchase = purchase_links.get(&row.id).unwrap_or(&empty_purchase);
                let whole = match row.invoice_direction {
                    InvoiceDirection::Sales => authorization.whole(),
                    InvoiceDirection::Purchase => purchase_whole(&authorization),
                };
                items.push(self.cut_invoice_row(
                    row,
                    sales,
                    purchase,
                    sales_matched,
                    purchase_matched,
                    whole,
                ));
            }
        }
        let mut triples = Vec::new();
        let mut whole_sum = zero_amount();
        let mut all_whole = true;
        for (row, sales_matched, purchase_matched) in decided.iter() {
            let sales = sales_links.get(&row.id).unwrap_or(&empty_sales);
            let purchase = purchase_links.get(&row.id).unwrap_or(&empty_purchase);
            let whole = match row.invoice_direction {
                InvoiceDirection::Sales => authorization.whole(),
                InvoiceDirection::Purchase => purchase_whole(&authorization),
            };
            all_whole &= whole;
            triples.extend(invoice_summary_inputs(sales, purchase, sales_matched, purchase_matched));
            if whole {
                whole_sum = whole_sum.checked_add(row.gross_amount);
            }
        }
        let mut owner_of = HashMap::new();
        for (id, fact) in sales_facts.iter() {
            owner_of.insert(id.clone(), fact.owner_user_id.clone());
        }
        for (id, fact) in purchase_facts.iter() {
            if let Some(owner) = fact.owner_user_id.clone() {
                owner_of.insert(id.clone(), owner);
            }
        }
        let version = format!("{:x}", fingerprint.finish());
        ensure_version(params.scope_version.as_deref(), &version).map_err(|_| changed())?;
        let summary =
            build_summary(&triples, &owner_of, whole_amount(all_whole, whole_sum), &version, !all_whole)?;
        let owner_options = self.owner_options_merged(&authorization, executor).await?;
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
            scope_summary: "发票按分配关联销售/采购当前负责人与登记经办人授权；部分授权仅返获授权份额",
            ownership_basis: "linked_sales_and_purchase_owner_and_register_operator",
        })
    }

    /// 销售与采购双边界的负责人候选合并；候选不授予命令资格。
    pub(super) async fn owner_options_merged(
        &self,
        authorization: &FundsAuthorization,
        executor: &mut dyn Executor,
    ) -> Result<Vec<FilterOption>> {
        let mut options = self.owner_options_sales(authorization, executor).await?;
        options.extend(self.owner_options_purchase(authorization, executor).await?);
        options.sort_by(|left, right| left.value.cmp(&right.value).then(left.label.cmp(&right.label)));
        options.dedup_by(|next, prev| next.value == prev.value);
        Ok(options)
    }

    /// 单行金额裁剪；整单金额与完整分配仅整单资格返回，否则为 null。
    pub(super) fn cut_invoice_row(
        &self,
        row: &InvoiceRow,
        sales: &[SalesInvoiceLink],
        purchase: &[PurchaseInvoiceLink],
        sales_matched: &[String],
        purchase_matched: &[String],
        whole: bool,
    ) -> ScopedInvoiceRow {
        let mut allocations: Vec<_> = if whole {
            sales.iter().map(|link| link.view.clone()).collect()
        } else {
            sales
                .iter()
                .filter(|link| {
                    link.order.as_ref().is_some_and(|order| sales_matched.iter().any(|id| id == order))
                })
                .map(|link| link.view.clone())
                .collect()
        };
        allocations.sort_by_key(|view| view.allocation_seq);
        let mut purchase_views: Vec<_> = if whole {
            purchase.iter().map(|link| link.view.clone()).collect()
        } else {
            purchase
                .iter()
                .filter(|link| {
                    link.order.as_ref().is_some_and(|order| purchase_matched.iter().any(|id| id == order))
                })
                .map(|link| link.view.clone())
                .collect()
        };
        purchase_views.sort_by_key(|view| view.allocation_seq);
        let allocated = sum_invoice_signed(sales, purchase);
        ScopedInvoiceRow {
            id: row.id.clone(),
            invoice_no: row.invoice_no.clone(),
            invoice_direction: row.invoice_direction,
            invoice_kind: row.invoice_kind,
            status: row.stable.status(),
            invoice_date: row.invoice_date,
            created_at: row.created_at,
            visible_allocated_share: sum_invoice_matched(sales, purchase, sales_matched, purchase_matched),
            gross_amount: whole_amount(whole, row.gross_amount),
            allocated_total: whole_amount(whole, allocated),
            unallocated_amount: whole_amount(whole, invoice_unallocated(row, allocated)),
            allocations: Some(allocations),
            purchase_allocations: Some(purchase_views),
            permission_limited: !whole,
        }
    }

    /// 发票详情同一事务内解析、取数与裁剪；版本绑定关联单据。
    pub(super) async fn load_invoice_detail(
        &self,
        id: &str,
        actor: &AuditActor,
        purchase_access: &PurchaseAccess,
        executor: &mut dyn Executor,
    ) -> Result<FundsScopedResult<ScopedInvoiceRow>> {
        use erp_finance::entity::receivable::InvoiceDirection;
        use erp_finance::repository::InvoiceFilter;
        let (access, authorization) =
            self.resolve_dual(actor, "invoice", "detail", purchase_access, executor).await?;
        let filter = InvoiceFilter {
            keyword_ids: None,
            invoice_ids: Some(vec![id.to_string()]),
            invoice_direction: None,
            invoice_kind: None,
            party_id: None,
            invoice_no: None,
            status: None,
            page: 1,
            page_size: 1,
            sort_by: None,
            sort_ascending: false,
        };
        let page = self.db.invoices().search_invoices(&filter, executor).await?;
        let row = page.items.into_iter().next().ok_or_else(|| Error::NotFound("发票不存在".into()))?;
        let sales_links = self.sales_invoice_matched_links(std::slice::from_ref(&row.id), executor).await?;
        let purchase_links =
            self.purchase_invoice_matched_links(std::slice::from_ref(&row.id), executor).await?;
        let (sales_facts, purchase_facts) =
            self.invoice_fact_maps(&sales_links, &purchase_links, executor).await?;
        let empty_sales: Vec<SalesInvoiceLink> = Vec::new();
        let empty_purchase: Vec<PurchaseInvoiceLink> = Vec::new();
        let sales = sales_links.get(&row.id).unwrap_or(&empty_sales);
        let purchase = purchase_links.get(&row.id).unwrap_or(&empty_purchase);
        let sales_tuples = invoice_sales_tuples(&row, sales, &sales_facts);
        let purchase_tuples = invoice_purchase_tuples(&row, purchase, &purchase_facts);
        let sales_allowed = self
            .authorized_sales_ids(&authorization, executor)
            .await?
            .map(|list| list.into_iter().collect::<BTreeSet<_>>());
        let purchase_scope = authorization.purchase_scope.clone().unwrap_or_default();
        let purchase_allowed = self
            .authorized_purchase_ids(purchase_access, &purchase_scope, executor)
            .await?
            .map(|list| list.into_iter().collect::<BTreeSet<_>>());
        let sales_matched = matched_orders(&sales_tuples, &sales_allowed);
        let purchase_matched = matched_orders(&purchase_tuples, &purchase_allowed);
        let whole = match row.invoice_direction {
            InvoiceDirection::Sales => authorization.whole(),
            InvoiceDirection::Purchase => purchase_whole(&authorization),
        };
        let mut combined = sales_tuples;
        combined.extend(purchase_tuples);
        let operators = vec![row.stable.created_by.clone()];
        let visible = row_visible(&access, &combined, &operators, &[])?;
        let unlinked = combined.iter().all(|(order, _, _, _, _)| order.is_none());
        let matched_any = !sales_matched.is_empty() || !purchase_matched.is_empty();
        if !keep_row(visible, whole, matched_any, sales.is_empty() && purchase.is_empty() || unlinked) {
            return Err(Error::NotFound("发票不存在".into()));
        }
        let data = self.cut_invoice_row(&row, sales, purchase, &sales_matched, &purchase_matched, whole);
        let mut parts = vec![format!("{}:{}", row.id, row.version)];
        for order in sales_matched.iter().filter_map(|id| sales_facts.get(id)) {
            parts.push(format!("{}:{}", order.owner_user_id, order.version));
        }
        for order in purchase_matched.iter().filter_map(|id| purchase_facts.get(id)) {
            parts.push(format!("{}:{:?}:{}", id, order.owner_user_id, order.version));
        }
        Ok(FundsScopedResult {
            data,
            scope_version: scope_version(&authorization.context, &parts),
            policy_version: authorization.context.policy_version,
            organization_version: authorization.context.organizations.version,
            as_of: authorization.context.as_of.as_utc().to_rfc3339(),
            empty_reason: None,
            scope_summary: "发票按分配关联销售/采购当前负责人与登记经办人授权；部分授权仅返获授权份额",
            ownership_basis: "linked_sales_and_purchase_owner_and_register_operator",
        })
    }
}

/// 发票销项分配关联元组；缺失订单的分配保留未分配归属，不丢份额。
pub(super) fn invoice_sales_tuples(
    row: &InvoiceRow,
    links: &[SalesInvoiceLink],
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

/// 发票进项分配关联元组；结算单来源与缺失订单计入未分配。
pub(super) fn invoice_purchase_tuples(
    row: &InvoiceRow,
    links: &[PurchaseInvoiceLink],
    facts: &HashMap<String, LinkedPurchaseFact>,
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
                fact.owner_user_id.clone(),
                Some(fact.business_org_unit_id.clone()),
                row.id.clone(),
                row.version,
            )),
            None => tuples.push((None, None, None, row.id.clone(), row.version)),
        }
    }
    tuples
}

/// 发票双方向条件匹配：销售负责人验销项侧，采购负责人验进项侧，经办人验登记人。
pub(super) fn matches_invoice_condition(
    sales: &[OrderTuple],
    purchase: &[OrderTuple],
    operators: &[String],
    condition: &FundsLinkedCondition,
) -> bool {
    if let Some(wanted) = &condition.operator_user_ids
        && !operators.iter().any(|id| wanted.iter().any(|item| item == id))
    {
        return false;
    }
    if let Some(wanted) = &condition.owner_user_ids
        && !sales
            .iter()
            .any(|(_, owner, _, _, _)| owner.as_ref().is_some_and(|id| wanted.iter().any(|item| item == id)))
    {
        return false;
    }
    if let Some(wanted) = &condition.secondary_operator_user_ids
        && !purchase
            .iter()
            .any(|(_, owner, _, _, _)| owner.as_ref().is_some_and(|id| wanted.iter().any(|item| item == id)))
    {
        return false;
    }
    if let Some(wanted) = &condition.org_unit_ids {
        let hit_sales = sales
            .iter()
            .any(|(_, _, org, _, _)| org.as_ref().is_some_and(|id| wanted.iter().any(|item| item == id)));
        let hit_purchase = purchase
            .iter()
            .any(|(_, _, org, _, _)| org.as_ref().is_some_and(|id| wanted.iter().any(|item| item == id)));
        if !hit_sales && !hit_purchase {
            return false;
        }
    }
    true
}

/// 发票业务归属筛选：来源销售单与应收子账只收窄授权结果。
pub(super) fn matches_invoice_business(
    query: &erp_finance::dto::receivable::InvoiceListQuery,
    row: &InvoiceRow,
    sales: &[SalesInvoiceLink],
    sales_matched: &[String],
) -> bool {
    use erp_finance::entity::receivable::InvoiceDirection;
    if let Some(wanted) = query.sales_order_id.as_deref()
        && row.invoice_direction == InvoiceDirection::Sales
        && !sales.iter().any(|link| link.order.as_deref() == Some(wanted))
    {
        return false;
    }
    if let Some(wanted) = query.receivable_account_id.as_ref()
        && !sales.iter().any(|link| {
            link.view.receivable_account_id == wanted.to_string() && link.order.as_ref().is_none_or(|_| true)
        })
    {
        return false;
    }
    if query.sales_order_id.is_some() && row.invoice_direction == InvoiceDirection::Purchase {
        return sales_matched.is_empty() && sales.is_empty();
    }
    true
}

/// 发票整单口径求和；仅整单读取资格持有者可见的结果使用，不得用于部分授权。
pub(super) fn sum_invoice_signed(sales: &[SalesInvoiceLink], purchase: &[PurchaseInvoiceLink]) -> Amount {
    let mut total = zero_amount();
    for link in sales {
        total = total.checked_add(link.signed);
    }
    for link in purchase {
        total = total.checked_add(link.signed);
    }
    total
}

/// 发票匹配份额求和；方向已在装载时按正反动作记入金额符号，不做差额推导。
pub(super) fn sum_invoice_matched(
    sales: &[SalesInvoiceLink],
    purchase: &[PurchaseInvoiceLink],
    sales_matched: &[String],
    purchase_matched: &[String],
) -> Amount {
    let mut total = zero_amount();
    for link in sales {
        if link.order.as_ref().is_none_or(|order| sales_matched.iter().any(|id| id == order)) {
            total = total.checked_add(link.signed);
        }
    }
    for link in purchase {
        if link.order.as_ref().is_none_or(|order| purchase_matched.iter().any(|id| id == order)) {
            total = total.checked_add(link.signed);
        }
    }
    total
}

/// 发票汇总输入：匹配份额与未分配份额进入汇总，未授权订单份额不得进入。
pub(super) fn invoice_summary_inputs(
    sales: &[SalesInvoiceLink],
    purchase: &[PurchaseInvoiceLink],
    sales_matched: &[String],
    purchase_matched: &[String],
) -> Vec<(String, Amount, LinkedOrderId)> {
    let mut inputs = Vec::new();
    for link in sales {
        if link.order.as_ref().is_none_or(|order| sales_matched.iter().any(|id| id == order)) {
            inputs.push((link.id.clone(), link.signed, link.order.clone()));
        }
    }
    for link in purchase {
        if link.order.as_ref().is_none_or(|order| purchase_matched.iter().any(|id| id == order)) {
            inputs.push((link.id.clone(), link.signed, link.order.clone()));
        }
    }
    inputs
}

/// 发票未分配余额沿用发票查询口径：蓝票含税减已分配，红票含税加已分配。
pub(super) fn invoice_unallocated(row: &InvoiceRow, allocated: Amount) -> Amount {
    use erp_finance::entity::receivable::InvoiceKind;
    match row.invoice_kind {
        InvoiceKind::Blue => row.gross_amount.checked_sub(allocated),
        InvoiceKind::Red => row.gross_amount.checked_add(allocated),
    }
}
