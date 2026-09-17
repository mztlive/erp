//! 供应商付款范围查询与金额裁剪。

use std::collections::{BTreeSet, HashMap};
use std::hash::{Hash, Hasher};

use application_core::AuditActor;
use erp_core::money::Amount;
use erp_finance::ports::funds_scope::FundsResolvedScope;
use erp_finance::repository::{PayableExt, SupplierPaymentFilter, SupplierPaymentRow};
use erp_procurement::PurchaseAccess;
use persistence_core::{Executor, Transactional};
use validator::Validate;

use super::allocation::purchase_whole;
use super::authorization::*;
use super::receipt::matched_orders;
use super::rows::*;
use crate::{Error, Result};

impl FundsAccess {
    /// 分页查询供应商付款范围行：采购负责人与付款经办人分别查询。
    pub async fn supplier_payment_list_scoped(
        &self,
        params: &erp_finance::dto::payable::SupplierPaymentListParams,
        actor: &AuditActor,
        purchase_access: &PurchaseAccess,
    ) -> Result<FundsScopedPage<ScopedSupplierPaymentRow>> {
        params.validate()?;
        let query = params.normalized().map_err(Error::from)?;
        ensure_page(query.paging.page, params.scope_version.as_deref())?;
        let snapshot = self
            .checked_supplier_payments(
                params,
                &query,
                actor,
                purchase_access,
                params.scope_version.as_deref(),
            )
            .await?;
        Ok(snapshot)
    }

    /// 独立详情重新解析付款详情动作；不可见与不存在统一为 NotFound。
    pub async fn supplier_payment_detail_scoped(
        &self,
        id: &str,
        actor: &AuditActor,
        purchase_access: &PurchaseAccess,
    ) -> Result<FundsScopedResult<ScopedSupplierPaymentRow>> {
        let this = self.clone();
        let actor = actor.clone();
        let id = id.to_string();
        let purchase_access = purchase_access.clone();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    this.load_supplier_payment_detail(&id, &actor, &purchase_access, executor).await
                })
            })
            .await
    }

    /// 返回前重读授权及候选事实版本；变化时拒绝交付原结果。
    pub(super) async fn checked_supplier_payments(
        &self,
        params: &erp_finance::dto::payable::SupplierPaymentListParams,
        query: &erp_finance::dto::payable::SupplierPaymentListQuery,
        actor: &AuditActor,
        purchase_access: &PurchaseAccess,
        expected: Option<&str>,
    ) -> Result<FundsScopedPage<ScopedSupplierPaymentRow>> {
        let snapshot = self.snapshot_supplier_payments(params, query, actor, purchase_access).await?;
        if expected.is_some_and(|value| value != snapshot.scope_version) {
            return Err(changed());
        }
        let current = self.snapshot_supplier_payments(params, query, actor, purchase_access).await?;
        if current.scope_version != snapshot.scope_version {
            return Err(changed());
        }
        Ok(snapshot)
    }

    /// 身份和业务事实均使用调用方同一事务，不缓存权限解析结果。
    pub(super) async fn snapshot_supplier_payments(
        &self,
        params: &erp_finance::dto::payable::SupplierPaymentListParams,
        query: &erp_finance::dto::payable::SupplierPaymentListQuery,
        actor: &AuditActor,
        purchase_access: &PurchaseAccess,
    ) -> Result<FundsScopedPage<ScopedSupplierPaymentRow>> {
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
                    this.load_supplier_payments(&params, &query, &actor, &purchase_access, executor).await
                })
            })
            .await
    }

    /// 分页查询付款范围行：核销关联采购当前负责人与付款经办人分别查询。
    pub(super) async fn load_supplier_payments(
        &self,
        params: &erp_finance::dto::payable::SupplierPaymentListParams,
        query: &erp_finance::dto::payable::SupplierPaymentListQuery,
        actor: &AuditActor,
        purchase_access: &PurchaseAccess,
        executor: &mut dyn Executor,
    ) -> Result<FundsScopedPage<ScopedSupplierPaymentRow>> {
        let (access, authorization) =
            self.resolve_with_purchase(actor, "supplier_payment", "list", purchase_access, executor).await?;
        if authorization.empty() {
            return Ok(empty_page(&authorization, "no_scope", "供应商付款无可见范围"));
        }
        let keyword_ids = crate::finance::search::keyword_ids(
            &self.db,
            query.q.as_deref(),
            erp_finance::repository::keyword::FinanceSearchTarget::Payment,
        )
        .await?;
        let filter = SupplierPaymentFilter {
            keyword_ids,
            keyword: None,
            keyword_supplier_ids: Vec::new(),
            payment_no: query.payment_no.clone(),
            supplier_id: query.supplier_id.clone(),
            status: query.status,
            page: 1,
            page_size: 10_000,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: matches!(query.paging.sort_dir, application_core::SortDir::Asc),
        };
        let candidates = self.db.supplier_payments().search_supplier_payments(&filter, executor).await?;
        if candidates.items.len() >= 10_000 {
            return Err(Error::ValidationError("付款查询超过上限，请收窄组织或负责人条件".into()));
        }
        let decided = self
            .assemble_supplier_payments(
                query,
                candidates.items,
                &access,
                &authorization,
                purchase_access,
                executor,
            )
            .await?;
        let ids = decided.iter().map(|(row, _)| row.id.clone()).collect::<Vec<_>>();
        let links = self.payment_matched_links(&ids, executor).await?;
        let order_ids = links.values().flatten().filter_map(|link| link.order.clone()).collect::<Vec<_>>();
        let facts = self.purchase_fact_map(&order_ids, executor).await?;
        let mut fingerprint = std::collections::hash_map::DefaultHasher::new();
        authorization.context.scope_version.hash(&mut fingerprint);
        for (row, matched) in decided.iter() {
            row.id.hash(&mut fingerprint);
            row.version.hash(&mut fingerprint);
            for order in matched.iter().filter_map(|id| facts.get(id)) {
                order.version.hash(&mut fingerprint);
            }
        }
        self.finish_supplier_payments(
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

    /// 付款经办人事实：创建与提交审计均计入；无过滤条件时不读审计。
    pub(super) async fn payment_operators(
        &self,
        ids: &[String],
        want: bool,
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, Vec<String>>> {
        if !want {
            return Ok(HashMap::new());
        }
        self.audit_operators(
            "supplier_payment",
            ids,
            |action| action == "supplier_payment.create" || action == "supplier_payment.commit",
            executor,
        )
        .await
    }

    /// 付款关联筛选条件：采购负责人、付款经办人与组织分别精确匹配。
    pub(super) async fn payment_condition(
        &self,
        query: &erp_finance::dto::payable::SupplierPaymentListQuery,
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
            owner_user_ids: query.procurement_owner_user_ids.as_ref().map(|ids| ids.as_slice().to_vec()),
            operator_user_ids: query.operator_user_ids.as_ref().map(|ids| ids.as_slice().to_vec()),
            secondary_operator_user_ids: None,
            org_unit_ids,
        })
    }

    /// 付款候选逐行判定可见性与筛选；授权集合外的关联份额不进入行与汇总。
    pub(super) async fn assemble_supplier_payments(
        &self,
        query: &erp_finance::dto::payable::SupplierPaymentListQuery,
        rows: Vec<SupplierPaymentRow>,
        access: &FundsResolvedScope,
        authorization: &FundsAuthorization,
        purchase_access: &PurchaseAccess,
        executor: &mut dyn Executor,
    ) -> Result<Vec<(SupplierPaymentRow, Vec<String>)>> {
        let ids = rows.iter().map(|row| row.id.clone()).collect::<Vec<_>>();
        let links = self.payment_matched_links(&ids, executor).await?;
        let order_ids = links.values().flatten().filter_map(|link| link.order.clone()).collect::<Vec<_>>();
        let facts = self.purchase_fact_map(&order_ids, executor).await?;
        let purchase_scope = authorization.purchase_scope.clone().unwrap_or_default();
        let allowed = self
            .authorized_purchase_ids(purchase_access, &purchase_scope, executor)
            .await?
            .map(|list| list.into_iter().collect::<BTreeSet<_>>());
        let operators = self.payment_operators(&ids, query.operator_user_ids.is_some(), executor).await?;
        let condition = self.payment_condition(query, executor).await?;
        let whole = purchase_whole(authorization);
        let mut decided = Vec::new();
        for row in rows {
            let empty_links: Vec<PaymentLink> = Vec::new();
            let row_links = links.get(&row.id).unwrap_or(&empty_links);
            let tuples = payment_tuples(&row, row_links, &facts);
            let matched = matched_orders(&tuples, &allowed);
            let doc_operators = operators.get(&row.id).cloned().unwrap_or_default();
            let visible = row_visible(access, &tuples, &doc_operators, &[])?;
            let unlinked = row_links.iter().all(|link| link.order.is_none());
            if !keep_row(visible, whole, !matched.is_empty(), row_links.is_empty() || unlinked) {
                continue;
            }
            if !matches_multi_condition(&tuples, &doc_operators, &[], &condition) {
                continue;
            }
            decided.push((row, matched));
        }
        Ok(decided)
    }

    /// 付款候选分页裁剪与汇总装配；明细、汇总与导出复用同一已决集合。
    // 查询+分页+执行器参数为既有签名，保持调用方一致不拆。
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn finish_supplier_payments(
        &self,
        params: &erp_finance::dto::payable::SupplierPaymentListParams,
        query: &erp_finance::dto::payable::SupplierPaymentListQuery,
        decided: Vec<(SupplierPaymentRow, Vec<String>)>,
        links: HashMap<String, Vec<PaymentLink>>,
        facts: HashMap<String, LinkedPurchaseFact>,
        authorization: FundsAuthorization,
        fingerprint: std::collections::hash_map::DefaultHasher,
        executor: &mut dyn Executor,
    ) -> Result<FundsScopedPage<ScopedSupplierPaymentRow>> {
        let total = decided.len() as u64;
        let page = query.paging.page.max(1);
        let page_size = u64::from(query.paging.page_size).max(1);
        let start = ((page - 1) as usize).saturating_mul(page_size as usize);
        let end = start.saturating_add(page_size as usize).min(decided.len());
        let whole = purchase_whole(&authorization);
        let empty_links: Vec<PaymentLink> = Vec::new();
        let mut items = Vec::new();
        if start < decided.len() {
            for (row, matched) in decided[start..end].iter() {
                let row_links = links.get(&row.id).unwrap_or(&empty_links);
                items.push(cut_payment_row(row, row_links, matched, whole));
            }
        }
        let mut triples = Vec::new();
        let mut whole_sum = zero_amount();
        for (row, matched) in decided.iter() {
            let row_links = links.get(&row.id).unwrap_or(&empty_links);
            triples.extend(payment_summary_inputs(row_links, matched));
            whole_sum = whole_sum.checked_add(row.amount);
        }
        let mut owner_of = HashMap::new();
        for (id, fact) in facts.iter() {
            if let Some(owner) = fact.owner_user_id.clone() {
                owner_of.insert(id.clone(), owner);
            }
        }
        let version = format!("{:x}", fingerprint.finish());
        ensure_version(params.scope_version.as_deref(), &version).map_err(|_| changed())?;
        let summary = build_summary(&triples, &owner_of, whole_amount(whole, whole_sum), &version, !whole)?;
        let owner_options = self.owner_options_purchase(&authorization, executor).await?;
        Ok(FundsScopedPage {
            items,
            total,
            summary,
            owner_options,
            page,
            page_size: page_size as u32,
            scope_version: version.clone(),
            policy_version: authorization.context.policy_version,
            organization_version: authorization.context.organizations.version,
            as_of: authorization.context.as_of.as_utc().to_rfc3339(),
            empty_reason: None,
            scope_summary: "付款按核销关联采购当前负责人与付款经办人授权；部分授权仅返获授权份额",
            ownership_basis: "linked_purchase_owner_and_payment_operator",
        })
    }

    /// 付款详情同一事务内解析、取数与裁剪；版本绑定关联采购单。
    pub(super) async fn load_supplier_payment_detail(
        &self,
        id: &str,
        actor: &AuditActor,
        purchase_access: &PurchaseAccess,
        executor: &mut dyn Executor,
    ) -> Result<FundsScopedResult<ScopedSupplierPaymentRow>> {
        let (access, authorization) = self
            .resolve_with_purchase(actor, "supplier_payment", "detail", purchase_access, executor)
            .await?;
        let filter = SupplierPaymentFilter {
            keyword_ids: Some(vec![id.to_string()]),
            keyword: None,
            keyword_supplier_ids: Vec::new(),
            payment_no: None,
            supplier_id: None,
            status: None,
            page: 1,
            page_size: 1,
            sort_by: None,
            sort_ascending: false,
        };
        let page = self.db.supplier_payments().search_supplier_payments(&filter, executor).await?;
        let row =
            page.items.into_iter().next().ok_or_else(|| Error::NotFound("供应商付款单不存在".into()))?;
        let links = self.payment_matched_links(std::slice::from_ref(&row.id), executor).await?;
        let order_ids = links.values().flatten().filter_map(|link| link.order.clone()).collect::<Vec<_>>();
        let facts = self.purchase_fact_map(&order_ids, executor).await?;
        let purchase_scope = authorization.purchase_scope.clone().unwrap_or_default();
        let allowed = self
            .authorized_purchase_ids(purchase_access, &purchase_scope, executor)
            .await?
            .map(|list| list.into_iter().collect::<BTreeSet<_>>());
        let empty_links: Vec<PaymentLink> = Vec::new();
        let row_links = links.get(&row.id).unwrap_or(&empty_links);
        let tuples = payment_tuples(&row, row_links, &facts);
        let matched = matched_orders(&tuples, &allowed);
        let whole = purchase_whole(&authorization);
        let operators = self.payment_operators(std::slice::from_ref(&row.id), true, executor).await?;
        let doc_operators = operators.get(&row.id).cloned().unwrap_or_default();
        let visible = row_visible(&access, &tuples, &doc_operators, &[])?;
        let unlinked = row_links.iter().all(|link| link.order.is_none());
        if !keep_row(visible, whole, !matched.is_empty(), row_links.is_empty() || unlinked) {
            return Err(Error::NotFound("供应商付款单不存在".into()));
        }
        let data = cut_payment_row(&row, row_links, &matched, whole);
        let mut parts = vec![format!("{}:{}", row.id, row.version)];
        for order in matched.iter().filter_map(|id| facts.get(id)) {
            parts.push(format!("{}:{:?}:{}", id, order.owner_user_id, order.version));
        }
        Ok(FundsScopedResult {
            data,
            scope_version: scope_version(&authorization.context, &parts),
            policy_version: authorization.context.policy_version,
            organization_version: authorization.context.organizations.version,
            as_of: authorization.context.as_of.as_utc().to_rfc3339(),
            empty_reason: None,
            scope_summary: "付款按核销关联采购当前负责人与付款经办人授权；部分授权仅返获授权份额",
            ownership_basis: "linked_purchase_owner_and_payment_operator",
        })
    }
}

/// 付款行关联元组；结算单来源与缺失订单的分配保留未分配归属，不丢份额。
pub(super) fn payment_tuples(
    row: &SupplierPaymentRow,
    links: &[PaymentLink],
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

/// 付款整单口径求和；仅整单读取资格持有者可见的结果使用，不得用于部分授权。
pub(super) fn payment_sum_all(links: &[PaymentLink]) -> Amount {
    let mut total = zero_amount();
    for link in links {
        total = total.checked_add(link.signed);
    }
    total
}

/// 付款匹配份额求和；无归属份额始终计入可见份额，不做差额推导。
pub(super) fn payment_sum_signed(links: &[PaymentLink], matched: &[String]) -> Amount {
    let mut total = zero_amount();
    for link in links {
        if link.order.as_ref().is_none_or(|order| matched.iter().any(|id| id == order)) {
            total = total.checked_add(link.signed);
        }
    }
    total
}

/// 付款汇总输入：匹配份额与未分配份额进入汇总，未授权采购单份额不得进入。
pub(super) fn payment_summary_inputs(
    links: &[PaymentLink],
    matched: &[String],
) -> Vec<(String, Amount, LinkedOrderId)> {
    links
        .iter()
        .filter(|link| link.order.as_ref().is_none_or(|order| matched.iter().any(|id| id == order)))
        .map(|link| (link.id.clone(), link.signed, link.order.clone()))
        .collect()
}

/// 付款单行裁剪；整单金额与完整分配仅整单资格返回，否则为 null。
pub(super) fn cut_payment_row(
    row: &SupplierPaymentRow,
    links: &[PaymentLink],
    matched: &[String],
    whole: bool,
) -> ScopedSupplierPaymentRow {
    let mut views: Vec<_> = if whole {
        links.iter().map(|link| link.view.clone()).collect()
    } else {
        links
            .iter()
            .filter(|link| link.order.as_ref().is_none_or(|order| matched.iter().any(|id| id == order)))
            .map(|link| link.view.clone())
            .collect()
    };
    views.sort_by_key(|view| view.allocation_seq);
    let net = payment_sum_all(links);
    ScopedSupplierPaymentRow {
        id: row.id.clone(),
        payment_no: row.payment_no.clone(),
        status: row.status,
        supplier_id: row.supplier_id.clone(),
        paid_at: row.paid_at,
        created_at: row.created_at,
        visible_allocated_share: payment_sum_signed(links, matched),
        amount: whole_amount(whole, row.amount),
        allocated_total: whole_amount(whole, net),
        unallocated_amount: whole_amount(whole, row.amount.checked_sub(net)),
        allocations: Some(views),
        permission_limited: !whole,
    }
}
