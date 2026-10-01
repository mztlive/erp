//! 销项发票范围查询与金额裁剪。

use std::collections::hash_map::DefaultHasher;
use std::collections::{BTreeSet, HashMap};
use std::hash::{Hash, Hasher};

use application_core::AuditActor;
use erp_core::money::Amount;
use erp_finance::dto::receivable::{InvoiceListParams, InvoiceListQuery, SalesInvoiceAllocationView};
use erp_finance::entity::read_coverage::whole_document_readable;
use erp_finance::entity::receivable::{InvoiceDirection, InvoiceKind, SalesInvoiceAllocation};
use erp_finance::repository::keyword::FinanceSearchTarget;
use erp_finance::repository::prelude::*;
use erp_finance::repository::{InvoiceFilter, InvoiceRow, ReceivableExt};
use erp_procurement::PurchaseAccess;
use persistence_core::{Executor, Transactional};
use validator::Validate;

use super::allocation::*;
use super::authorization::*;
use super::receipt::matched_orders;
use super::repository::invoice::{InvoiceSnapshot, InvoiceVersion, invoice_snapshot};
use super::rows::*;
use crate::finance::search::keyword_ids;
use crate::{Error, Result};

/// 销项发票分配实体转响应视图；与发票查询服务保持同一字段口径。
///
/// # 参数
/// 原销项分配实体。
/// # 返回
/// 原分配动作、引用和金额字段的响应视图。
/// # 错误
/// 无。
pub(super) fn sales_invoice_allocation_view(item: &SalesInvoiceAllocation) -> SalesInvoiceAllocationView {
    SalesInvoiceAllocationView {
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

impl FundsAccess {
    /// 分页查询发票范围行：销项按负责销售，登记经办人与组织分别查询。
    ///
    /// # 参数
    /// 请求参数、当前调用人和采购范围服务。
    /// # 返回
    /// 最终授权份额的分页响应与汇总。
    /// # 错误
    /// 参数、范围版本、授权、持久化或查询上限检查失败时拒绝。
    pub async fn invoice_list_scoped(
        &self,
        params: &InvoiceListParams,
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
    ///
    /// # 参数
    /// 发票主键、当前调用人和采购范围服务。
    /// # 返回
    /// 当前授权资格裁剪的发票详情。
    /// # 错误
    /// 不可见或不存在返回 NotFound；其他读取错误原样传播。
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
    ///
    /// # 参数
    /// 请求与规范化查询、调用人、采购范围服务和预期版本。
    /// # 返回
    /// 独立轻量完整复核通过后的首拍页面。
    /// # 错误
    /// 首拍预期版本或二拍完整范围版本不一致时拒绝。
    pub(super) async fn checked_invoices(
        &self,
        params: &InvoiceListParams,
        query: &InvoiceListQuery,
        actor: &AuditActor,
        purchase_access: &PurchaseAccess,
        expected: Option<&str>,
    ) -> Result<FundsScopedPage<ScopedInvoiceRow>> {
        checked_revalidated(
            expected,
            || self.snapshot_invoices(params, query, actor, purchase_access),
            || self.invoice_fingerprint_snapshot(query, actor, purchase_access),
        )
        .await
    }

    /// 身份和业务事实均使用调用方同一事务，不缓存权限解析结果。
    ///
    /// # 参数
    /// 请求与规范化查询、调用人和采购范围服务。
    /// # 返回
    /// 同事务内形成的最终授权发票页面。
    /// # 错误
    /// 授权解析、持久化或汇总读取失败时拒绝。
    pub(super) async fn snapshot_invoices(
        &self,
        params: &InvoiceListParams,
        query: &InvoiceListQuery,
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
    /// 数据库先形成最终授权份额，再查询页面、总数、完整版本与金额流。
    ///
    /// # 参数
    /// 请求与规范化查询、调用人、采购范围服务和原快照执行器。
    /// # 返回
    /// 数据库分页、同口径计数及授权份额汇总。
    /// # 错误
    /// 读取、解码、范围版本或查询上限检查失败时拒绝。
    pub(super) async fn load_invoices(
        &self,
        params: &InvoiceListParams,
        query: &InvoiceListQuery,
        actor: &AuditActor,
        purchase_access: &PurchaseAccess,
        executor: &mut dyn Executor,
    ) -> Result<FundsScopedPage<ScopedInvoiceRow>> {
        let (authorization, snapshot) =
            self.invoice_projection(query, actor, purchase_access, true, executor).await?;
        if authorization.empty() {
            return Ok(empty_page(&authorization, "no_scope", "发票无可见范围"));
        }
        let version = invoice_scope_version(&authorization, &snapshot.versions);
        ensure_version(params.scope_version.as_deref(), &version).map_err(|_| changed())?;
        let summary = snapshot.summary(&version)?;
        let items = snapshot
            .items
            .iter()
            .map(|projected| {
                let (sales, purchase) = projected.links();
                self.cut_invoice_row(
                    &projected.row,
                    &sales,
                    &purchase,
                    &projected.sales_matched,
                    &projected.purchase_matched,
                    projected.whole,
                )
            })
            .collect();
        Ok(FundsScopedPage {
            items,
            total: snapshot.total(),
            summary,
            page: query.paging.page,
            page_size: query.paging.page_size,
            scope_version: version,
            policy_version: authorization.context.policy_version,
            organization_version: authorization.context.organizations.version,
            as_of: authorization.context.as_of.as_utc().to_rfc3339(),
            empty_reason: None,
            scope_summary: "发票按销售、采购或结算来源分配授权；完整票面另需财务整账职责及全部来源可见",
            ownership_basis: "linked_sales_and_purchase_owner_and_register_operator",
        })
    }

    /// 两拍独立解析当前授权、搜索和组织条件，只复用同一拍的事实。
    async fn invoice_projection(
        &self,
        query: &InvoiceListQuery,
        actor: &AuditActor,
        purchase_access: &PurchaseAccess,
        materialize: bool,
        executor: &mut dyn Executor,
    ) -> Result<(FundsAuthorization, InvoiceSnapshot)> {
        let (_, authorization) =
            self.resolve_dual(actor, "invoice", "list", purchase_access, executor).await?;
        if authorization.empty() {
            return Ok((authorization, InvoiceSnapshot::default()));
        }
        let target = match query.invoice_direction {
            Some(InvoiceDirection::Purchase) => FinanceSearchTarget::PurchaseInvoice,
            _ => FinanceSearchTarget::SalesInvoice,
        };
        let keyword_ids = keyword_ids(&self.db, query.q.as_deref(), target).await?;
        let filter = InvoiceFilter {
            keyword_ids,
            invoice_direction: query.invoice_direction,
            invoice_kind: query.invoice_kind,
            party_id: query.party_id.clone(),
            invoice_no: query.invoice_no.clone(),
            status: query.status,
            ..Default::default()
        };
        let condition = self.invoice_condition(query, executor).await?;
        let snapshot =
            invoice_snapshot(&self.db, &filter, query, &authorization, &condition, materialize, executor)
                .await?;
        Ok((authorization, snapshot))
    }

    /// 第二拍在独立事务读取完整匹配版本，不装配页面或汇总金额。
    async fn invoice_fingerprint_snapshot(
        &self,
        query: &InvoiceListQuery,
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
                    let (authorization, snapshot) =
                        this.invoice_projection(&query, &actor, &purchase_access, false, executor).await?;
                    if authorization.empty() {
                        return Ok(authorization.context.scope_version);
                    }
                    Ok(invoice_scope_version(&authorization, &snapshot.versions))
                })
            })
            .await
    }

    /// 发票双方向关联事实一次取回；缺失订单保留未分配归属。
    ///
    /// # 参数
    /// 两方向分配关联集合和原执行器。
    /// # 返回
    /// 当前销售与采购或结算来源责任事实。
    /// # 错误
    /// 持久化读取失败时返回错误。
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

    /// 发票关联筛选条件：销售负责人、采购负责人、登记经办人与组织分别精确匹配。
    ///
    /// # 参数
    /// 规范化查询和原执行器。
    /// # 返回
    /// 负责人、登记人及展开组织的精确筛选条件。
    /// # 错误
    /// 组织展开读取失败时返回错误。
    pub(super) async fn invoice_condition(
        &self,
        query: &InvoiceListQuery,
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

    /// 单行金额裁剪；整单金额与完整分配仅整单资格返回，否则为 null。
    ///
    /// # 参数
    /// 票面、实际分配、匹配来源集合及整票资格。
    /// # 返回
    /// 按授权份额裁剪的原发票行视图。
    /// # 错误
    /// 无；金额越界保留原金额类型行为。
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
    ///
    /// # 参数
    /// 发票主键、调用人、采购范围服务和原执行器。
    /// # 返回
    /// 已重新解析详情动作的授权份额视图。
    /// # 错误
    /// 不存在、不可见或必要来源缺失时拒绝。
    pub(super) async fn load_invoice_detail(
        &self,
        id: &str,
        actor: &AuditActor,
        purchase_access: &PurchaseAccess,
        executor: &mut dyn Executor,
    ) -> Result<FundsScopedResult<ScopedInvoiceRow>> {
        let (_access, authorization) =
            self.resolve_dual(actor, "invoice", "detail", purchase_access, executor).await?;
        let filter =
            InvoiceFilter { invoice_ids: Some(vec![id.to_string()]), page_size: 1, ..Default::default() };
        let page = self.db.invoices().search_invoices(&filter, executor).await?;
        let row = page.items.into_iter().next().ok_or_else(|| Error::NotFound("发票不存在".into()))?;
        let sales_links = self.sales_invoice_matched_links(std::slice::from_ref(&row.id), executor).await?;
        let purchase_links =
            self.purchase_invoice_matched_links(std::slice::from_ref(&row.id), executor).await?;
        let (sales_facts, purchase_facts) =
            self.invoice_fact_maps(&sales_links, &purchase_links, executor).await?;
        let sales = sales_links.get(&row.id).map(Vec::as_slice).unwrap_or_default();
        let purchase = purchase_links.get(&row.id).map(Vec::as_slice).unwrap_or_default();
        if !linked_sources_exist(sales.iter().map(|link| link.order.as_deref()), &sales_facts)
            || !linked_sources_exist(purchase.iter().map(|link| link.order.as_deref()), &purchase_facts)
        {
            return Err(Error::NotFound("发票不存在".into()));
        }
        let sales_tuples = invoice_sales_tuples(&row, sales, &sales_facts);
        let purchase_tuples = invoice_purchase_tuples(&row, purchase, &purchase_facts);
        let sales_allowed = self
            .authorized_sales_ids(&authorization, executor)
            .await?
            .map(|list| list.into_iter().collect::<BTreeSet<_>>());
        let purchase_allowed = authorization.payable_ids(&purchase_facts);
        let sales_matched = matched_orders(&sales_tuples, &sales_allowed);
        let purchase_matched = matched_orders(&purchase_tuples, &purchase_allowed);
        let whole = whole_invoice_readable(
            authorization.ledger_read,
            sales,
            purchase,
            &sales_matched,
            &purchase_matched,
        );
        if !whole && sales_matched.is_empty() && purchase_matched.is_empty() {
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
        Ok(invoice_detail_result(&authorization, data, &parts))
    }
}

/// 详情沿原授权上下文封装范围元信息，不改变明细独立授权或版本材料。
fn invoice_detail_result(
    authorization: &FundsAuthorization,
    data: ScopedInvoiceRow,
    parts: &[String],
) -> FundsScopedResult<ScopedInvoiceRow> {
    FundsScopedResult {
        data,
        scope_version: scope_version(&authorization.context, parts),
        policy_version: authorization.context.policy_version,
        organization_version: authorization.context.organizations.version,
        as_of: authorization.context.as_of.as_utc().to_rfc3339(),
        empty_reason: None,
        scope_summary: "发票按销售、采购或结算来源分配授权；完整票面另需财务整账职责及全部来源可见",
        ownership_basis: "linked_sales_and_purchase_owner_and_register_operator",
    }
}

/// 发票销项分配关联元组；缺失订单的分配保留未分配归属，不丢份额。
///
/// # 参数
/// 销项关联、当前销售事实和匹配来源集合。
/// # 返回
/// 既有格式的销售来源责任元组。
/// # 错误
/// 无。
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
///
/// # 参数
/// 进项关联、当前采购或结算事实和匹配来源集合。
/// # 返回
/// 既有格式的采购与结算责任元组。
/// # 错误
/// 无。
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

/// 发票整单口径求和；仅整单读取资格持有者可见的结果使用，不得用于部分授权。
///
/// # 参数
/// 两方向实际分配关联。
/// # 返回
/// 按原销项再进项顺序归并的方向金额。
/// # 错误
/// 金额越界保留原金额类型行为。
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
///
/// # 参数
/// 两方向实际分配与匹配来源集合。
/// # 返回
/// 按原顺序归并的可见份额金额。
/// # 错误
/// 金额越界保留原金额类型行为。
pub(super) fn sum_invoice_matched(
    sales: &[SalesInvoiceLink],
    purchase: &[PurchaseInvoiceLink],
    sales_matched: &[String],
    purchase_matched: &[String],
) -> Amount {
    let mut total = zero_amount();
    for link in sales {
        if link.order.as_ref().is_some_and(|order| sales_matched.iter().any(|id| id == order)) {
            total = total.checked_add(link.signed);
        }
    }
    for link in purchase {
        if link.order.as_ref().is_some_and(|order| purchase_matched.iter().any(|id| id == order)) {
            total = total.checked_add(link.signed);
        }
    }
    total
}

/// 发票汇总输入：匹配份额与未分配份额进入汇总，未授权订单份额不得进入。
#[cfg(test)]
///
/// # 参数
/// 两方向实际分配与匹配来源集合。
/// # 返回
/// 保持原金额顺序的已授权汇总输入。
/// # 错误
/// 无。
pub(super) fn invoice_summary_inputs(
    sales: &[SalesInvoiceLink],
    purchase: &[PurchaseInvoiceLink],
    sales_matched: &[String],
    purchase_matched: &[String],
) -> Vec<(String, Amount, LinkedOrderId)> {
    let mut inputs = Vec::new();
    for link in sales {
        if link.order.as_ref().is_none_or(|order| sales_matched.iter().any(|id| id == order)) {
            inputs.push((link.view.id.clone(), link.signed, link.order.clone()));
        }
    }
    for link in purchase {
        if link.order.as_ref().is_none_or(|order| purchase_matched.iter().any(|id| id == order)) {
            inputs.push((link.view.id.clone(), link.signed, link.order.clone()));
        }
    }
    inputs
}

/// 发票未分配余额沿用发票查询口径：蓝票含税减已分配，红票含税加已分配。
///
/// # 参数
/// 原票面和完整分配方向金额。
/// # 返回
/// 按蓝票或红票规则计算的未分配金额。
/// # 错误
/// 金额越界保留原金额类型行为。
pub(super) fn invoice_unallocated(row: &InvoiceRow, allocated: Amount) -> Amount {
    match row.invoice_kind {
        InvoiceKind::Blue => row.gross_amount.checked_sub(allocated),
        InvoiceKind::Red => row.gross_amount.checked_add(allocated),
    }
}

/// 整张发票必须覆盖销项与进项实际分配，部分来源不授予完整票面。
///
/// # 参数
/// 整账资格、两方向实际分配及匹配来源集合。
/// # 返回
/// 全部实际来源可读时返回 true。
/// # 错误
/// 无。
pub(super) fn whole_invoice_readable(
    ledger_read: bool,
    sales: &[SalesInvoiceLink],
    purchase: &[PurchaseInvoiceLink],
    sales_matched: &[String],
    purchase_matched: &[String],
) -> bool {
    whole_document_readable(ledger_read, sales.iter().map(|link| link.order.as_deref()), sales_matched)
        && whole_document_readable(
            ledger_read,
            purchase.iter().map(|link| link.order.as_deref()),
            purchase_matched,
        )
}

/// 指纹沿原完整结果顺序绑定全部匹配票和真实来源版本。
fn invoice_scope_version(authorization: &FundsAuthorization, versions: &[InvoiceVersion]) -> String {
    let mut fingerprint = DefaultHasher::new();
    authorization.context.scope_version.hash(&mut fingerprint);
    for row in versions {
        row.hash_into(&mut fingerprint);
    }
    format!("{:x}", fingerprint.finish())
}

#[cfg(test)]
mod tests {
    use erp_finance::entity::receivable::AllocationAction;

    use super::*;

    fn sales_link(id: &str, order: Option<&str>, amount: &str) -> SalesInvoiceLink {
        let amount = amount.parse().unwrap();
        SalesInvoiceLink {
            order: order.map(str::to_string),
            signed: amount,
            view: SalesInvoiceAllocationView {
                id: id.into(),
                allocation_seq: 1,
                allocation_action: AllocationAction::Apply,
                receivable_account_id: id.into(),
                allocated_gross_amount: amount,
                allocated_net_amount: amount,
                allocated_tax_amount: zero_amount(),
                reverses_allocation_id: None,
            },
        }
    }

    #[test]
    fn invoice_share_and_partial_summary_exclude_other_orders_and_unknown_ownership() {
        let sales = vec![
            sales_link("a", Some("so-a"), "60"),
            sales_link("b", Some("so-b"), "40"),
            sales_link("unknown", None, "10"),
        ];
        let matched = vec!["so-a".into()];
        assert_eq!(sum_invoice_matched(&sales, &[], &matched, &[]), "60".parse().unwrap());
        let summary = build_summary(
            &invoice_summary_inputs(&sales, &[], &matched, &[]),
            &HashMap::from([("so-a".into(), "a".into())]),
            None,
            "v",
            true,
        )
        .unwrap();
        assert_eq!(summary.grouped.len(), 1);
        assert_eq!(summary.grouped[0].visible_share, "60".parse().unwrap());
        assert_eq!(summary.unassigned, zero_amount());
    }
}
