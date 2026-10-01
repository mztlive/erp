//! 供应商付款范围查询与金额裁剪。

use std::collections::{BTreeSet, HashMap};

use application_core::AuditActor;
use erp_core::money::Amount;
use erp_finance::dto::payable::SupplierPaymentListQuery;
use erp_finance::entity::read_coverage::whole_document_readable;
use erp_finance::repository::keyword::FinanceSearchTarget;
use erp_finance::repository::prelude::*;
use erp_finance::repository::{PayableExt, SupplierPaymentFilter, SupplierPaymentRow};
use erp_procurement::PurchaseAccess;
use persistence_core::{Executor, Transactional};
use validator::Validate;

use super::authorization::*;
use super::receipt::matched_orders;
use super::repository::{flow, payment as payment_repository};
use super::rows::*;
use crate::finance::search::keyword_ids;
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
        checked_revalidated(
            expected,
            || self.snapshot_supplier_payments(params, query, actor, purchase_access),
            || self.revalidate_supplier_payments(query, actor, purchase_access),
        )
        .await
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

    /// 最终来源份额授权与业务条件在数据库形成后分页，页外只返回窄摘要和版本。
    pub(super) async fn load_supplier_payments(
        &self,
        params: &erp_finance::dto::payable::SupplierPaymentListParams,
        query: &SupplierPaymentListQuery,
        actor: &AuditActor,
        purchase_access: &PurchaseAccess,
        executor: &mut dyn Executor,
    ) -> Result<FundsScopedPage<ScopedSupplierPaymentRow>> {
        let (_, authorization) =
            self.resolve_with_purchase(actor, "supplier_payment", "list", purchase_access, executor).await?;
        if authorization.empty() {
            return Ok(empty_page(&authorization, "no_scope", "供应商付款无可见范围"));
        }
        let condition = self.payment_condition(query, executor).await?;
        let filter = self.payment_filter(query).await?;
        let snapshot =
            payment_repository::page(&self.db, query, &filter, &authorization, &condition, executor).await?;
        let total = snapshot.count()?;
        let version = flow::version(&authorization.context.scope_version, &snapshot.versions);
        ensure_version(params.scope_version.as_deref(), &version).map_err(|_| changed())?;
        let summary = flow::summary(&snapshot.summary, &version)?;
        let items = self
            .payment_page(snapshot.items, &snapshot.versions, authorization.ledger_read, executor)
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
            scope_summary: "付款按核销关联采购或结算来源授权；整单另需财务整账职责及全部来源可见",
            ownership_basis: "linked_purchase_owner_and_payment_operator",
        })
    }

    /// 关键词与主表条件保持既有合同，关联负责人和组织在最终聚合筛选。
    async fn payment_filter(&self, query: &SupplierPaymentListQuery) -> Result<SupplierPaymentFilter> {
        Ok(SupplierPaymentFilter {
            keyword_ids: keyword_ids(&self.db, query.q.as_deref(), FinanceSearchTarget::Payment).await?,
            payment_no: query.payment_no.clone(),
            supplier_id: query.supplier_id.clone(),
            status: query.status,
            ..Default::default()
        })
    }

    /// 新事务只返回完整候选责任版本，不复算第二份金额汇总和页面。
    async fn revalidate_supplier_payments(
        &self,
        query: &SupplierPaymentListQuery,
        actor: &AuditActor,
        purchase_access: &PurchaseAccess,
    ) -> Result<String> {
        let this = self.clone();
        let query = query.clone();
        let actor = actor.clone();
        let purchase_access = purchase_access.clone();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let (_, authorization) = this
                        .resolve_with_purchase(&actor, "supplier_payment", "list", &purchase_access, executor)
                        .await?;
                    if authorization.empty() {
                        return Ok(authorization.context.scope_version);
                    }
                    let condition = this.payment_condition(&query, executor).await?;
                    let filter = this.payment_filter(&query).await?;
                    let versions = payment_repository::versions(
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
                            "付款查询超过上限，请收窄组织或负责人条件".into(),
                        ));
                    }
                    Ok(flow::version(&authorization.context.scope_version, &versions))
                })
            })
            .await
    }

    /// 仅当前页读取完整分配，复用正式金额裁剪规则和同拍匹配来源。
    async fn payment_page(
        &self,
        rows: Vec<SupplierPaymentRow>,
        versions: &[flow::FlowVersion],
        ledger_read: bool,
        executor: &mut dyn Executor,
    ) -> Result<Vec<ScopedSupplierPaymentRow>> {
        let ids = rows.iter().map(|row| row.id.clone()).collect::<Vec<_>>();
        let links = self.payment_matched_links(&ids, executor).await?;
        let source_ids = links.values().flatten().filter_map(|link| link.order.clone()).collect::<Vec<_>>();
        let facts = self.purchase_fact_map(&source_ids, executor).await?;
        let allowed = Some(
            versions.iter().flat_map(|row| row.sources.iter().map(|source| source.id.clone())).collect(),
        );
        let expected = rows.len();
        let decided = decide_payment_rows(rows, &links, &facts, &allowed, ledger_read);
        if decided.len() != expected {
            return Err(Error::Internal("付款来源聚合与正式裁剪规则不一致".into()));
        }
        Ok(payment_page_items(&decided, &links, ledger_read))
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

    /// 付款详情同一事务内解析、取数与裁剪；版本绑定关联采购单。
    ///
    /// # 参数
    /// 付款身份、操作人、采购读取访问器与调用方事务执行器。
    /// # 返回
    /// 返回当前获授权付款份额及来源版本凭据。
    /// # 错误
    /// 不存在或不可见统一为 NotFound；授权和持久化失败时拒绝。
    pub(super) async fn load_supplier_payment_detail(
        &self,
        id: &str,
        actor: &AuditActor,
        purchase_access: &PurchaseAccess,
        executor: &mut dyn Executor,
    ) -> Result<FundsScopedResult<ScopedSupplierPaymentRow>> {
        let (_access, authorization) = self
            .resolve_with_purchase(actor, "supplier_payment", "detail", purchase_access, executor)
            .await?;
        let filter = SupplierPaymentFilter {
            keyword_ids: Some(vec![id.to_string()]),
            page_size: 1,
            ..Default::default()
        };
        let page = self.db.supplier_payments().search_supplier_payments(&filter, executor).await?;
        let row =
            page.items.into_iter().next().ok_or_else(|| Error::NotFound("供应商付款单不存在".into()))?;
        let links = self.payment_matched_links(std::slice::from_ref(&row.id), executor).await?;
        let order_ids = links.values().flatten().filter_map(|link| link.order.clone()).collect::<Vec<_>>();
        let facts = self.purchase_fact_map(&order_ids, executor).await?;
        let allowed = authorization.payable_ids(&facts);
        let row_links = links.get(&row.id).map(Vec::as_slice).unwrap_or_default();
        if !linked_sources_exist(row_links.iter().map(|link| link.order.as_deref()), &facts) {
            return Err(Error::NotFound("供应商付款单不存在".into()));
        }
        let tuples = payment_tuples(&row, row_links, &facts);
        let matched = matched_orders(&tuples, &allowed);
        let whole = whole_document_readable(
            authorization.ledger_read,
            row_links.iter().map(|link| link.order.as_deref()),
            &matched,
        );
        if !whole && matched.is_empty() {
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
            scope_summary: "付款按核销关联采购或结算来源授权；整单另需财务整账职责及全部来源可见",
            ownership_basis: "linked_purchase_owner_and_payment_operator",
        })
    }
}

/// 当前页复用完整来源存在及整单覆盖，来源集合由数据库最终条件提供。
fn decide_payment_rows(
    rows: Vec<SupplierPaymentRow>,
    links: &HashMap<String, Vec<PaymentLink>>,
    facts: &HashMap<String, LinkedPurchaseFact>,
    allowed: &Option<BTreeSet<String>>,
    ledger_read: bool,
) -> Vec<(SupplierPaymentRow, Vec<String>)> {
    rows.into_iter()
        .filter_map(|row| {
            let lines = links.get(&row.id).map(Vec::as_slice).unwrap_or_default();
            if !linked_sources_exist(lines.iter().map(|link| link.order.as_deref()), facts) {
                return None;
            }
            let matched = matched_orders(&payment_tuples(&row, lines, facts), allowed);
            let whole = whole_document_readable(
                ledger_read,
                lines.iter().map(|link| link.order.as_deref()),
                &matched,
            );
            (whole || !matched.is_empty()).then_some((row, matched))
        })
        .collect()
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
        if link.order.as_ref().is_some_and(|order| matched.iter().any(|id| id == order)) {
            total = total.checked_add(link.signed);
        }
    }
    total
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
            .filter(|link| link.order.as_ref().is_some_and(|order| matched.iter().any(|id| id == order)))
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

/// 只对当前分页已授权行投影金额，沿用逐单据整账覆盖判定。
fn payment_page_items(
    rows: &[(SupplierPaymentRow, Vec<String>)],
    links: &HashMap<String, Vec<PaymentLink>>,
    ledger_read: bool,
) -> Vec<ScopedSupplierPaymentRow> {
    rows.iter()
        .map(|(row, matched)| {
            let row_links = links.get(&row.id).map(Vec::as_slice).unwrap_or_default();
            let whole = whole_document_readable(
                ledger_read,
                row_links.iter().map(|link| link.order.as_deref()),
                matched,
            );
            cut_payment_row(row, row_links, matched, whole)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use erp_core::common::time::Instant;
    use erp_finance::dto::payable::PaymentAllocationView;
    use erp_finance::entity::payable::{AllocationAction, SupplierPaymentStatus};

    use super::*;

    fn row(id: &str, amount: &str) -> SupplierPaymentRow {
        SupplierPaymentRow {
            id: id.into(),
            payment_no: "P".into(),
            status: SupplierPaymentStatus::Draft,
            supplier_id: "supplier".into(),
            paid_at: 1,
            amount: amount.parse().unwrap(),
            bank_reference: None,
            version: 1,
            created_at: 1,
        }
    }

    fn link(id: &str, order: Option<&str>, amount: &str) -> PaymentLink {
        let amount = amount.parse().unwrap();
        PaymentLink {
            order: order.map(str::to_string),
            signed: amount,
            view: PaymentAllocationView {
                id: id.into(),
                allocation_seq: 1,
                allocation_action: AllocationAction::Apply,
                payable_entry_id: id.into(),
                payable_account_id: None,
                source_type: None,
                source_document_id: None,
                source_document_no: None,
                allocated_amount: amount,
                allocated_at: Instant::from_unix_secs(1),
                reverses_allocation_id: None,
            },
        }
    }

    #[test]
    fn partial_payment_hides_other_and_unknown_allocations_and_all_whole_amounts() {
        let row = row("p", "120");
        let links =
            vec![link("a", Some("po-a"), "60"), link("b", Some("po-b"), "40"), link("unknown", None, "10")];
        let matched = vec!["po-a".into()];
        let partial = cut_payment_row(&row, &links, &matched, false);
        assert_eq!(partial.visible_allocated_share, "60".parse().unwrap());
        assert!(partial.amount.is_none());
        assert!(partial.allocated_total.is_none());
        assert!(partial.unallocated_amount.is_none());
        assert_eq!(
            partial.allocations.unwrap().iter().map(|item| item.id.as_str()).collect::<Vec<_>>(),
            vec!["a"]
        );
        let whole = cut_payment_row(&row, &links, &matched, true);
        assert_eq!(whole.visible_allocated_share, "60".parse().unwrap());
        assert_eq!(whole.amount, Some("120".parse().unwrap()));
        assert_eq!(whole.allocations.unwrap().len(), 3);
        assert_eq!(whole.unallocated_amount, Some("10".parse().unwrap()));
    }

    #[test]
    fn page_decision_distinguishes_dangling_sources_from_true_zero_allocations() {
        let allowed = Some(BTreeSet::new());
        assert!(
            decide_payment_rows(vec![row("empty", "10")], &HashMap::new(), &HashMap::new(), &allowed, false)
                .is_empty()
        );
        let readable =
            decide_payment_rows(vec![row("empty", "10")], &HashMap::new(), &HashMap::new(), &allowed, true);
        assert_eq!(readable.len(), 1);
        let dangling = HashMap::from([("p".into(), vec![link("a", Some("missing"), "10")])]);
        assert!(
            decide_payment_rows(vec![row("p", "10")], &dangling, &HashMap::new(), &allowed, true).is_empty()
        );
        let missing_reference = HashMap::from([("p".into(), vec![link("a", None, "10")])]);
        assert!(
            decide_payment_rows(vec![row("p", "10")], &missing_reference, &HashMap::new(), &allowed, true)
                .is_empty()
        );
    }

    #[test]
    fn database_matched_sources_keep_partial_page_amount_and_original_reverse_sequence() {
        let mut reverse = link("reverse", Some("po-a"), "1");
        reverse.view.allocation_action = AllocationAction::Reverse;
        reverse.signed = zero_amount().checked_sub(reverse.signed);
        let max = "79228162514264337593543950335";
        let links = HashMap::from([(
            "p".into(),
            vec![
                link("max", Some("po-a"), max),
                reverse,
                link("one", Some("po-a"), "1"),
                link("hidden", Some("po-b"), "0"),
            ],
        )]);
        let facts = ["po-a", "po-b"]
            .into_iter()
            .map(|id| {
                (
                    id.into(),
                    LinkedPurchaseFact {
                        owner_user_id: Some(id.into()),
                        business_org_unit_id: "org".into(),
                        version: 1,
                        document_no: id.into(),
                    },
                )
            })
            .collect();
        let allowed = Some(BTreeSet::from(["po-a".into()]));
        let decided = decide_payment_rows(vec![row("p", max)], &links, &facts, &allowed, true);
        assert_eq!(decided[0].1, ["po-a"]);
        let items = payment_page_items(&decided, &links, true);
        assert_eq!(items[0].visible_allocated_share, max.parse().unwrap());
        assert!(items[0].amount.is_none());
        assert!(items[0].permission_limited);
        assert_eq!(items[0].allocations.as_ref().unwrap().len(), 3);
    }
}
