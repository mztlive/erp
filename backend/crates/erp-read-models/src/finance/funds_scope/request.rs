//! 开票申请范围查询。

use std::collections::{BTreeSet, HashMap};
use std::hash::{Hash, Hasher};

use application_core::AuditActor;
use erp_core::ids::SalesOrderId;
use erp_finance::entity::receivable::SalesInvoiceRequest;
use erp_finance::ports::funds_scope::FundsResolvedScope;
use erp_finance::repository::ReceivableExt;
use erp_finance::repository::prelude::*;
use erp_sales::repository::SalesOrderExt;
use erp_sales::repository::prelude::*;
use persistence_core::{Executor, Transactional};

use super::authorization::*;
use super::rows::*;
use crate::{Error, Result};

impl FundsAccess {
    /// 分页查询开票申请范围行：负责销售/申请人/当前开票处理人分别查询。
    pub async fn request_list_scoped(
        &self,
        query: &erp_finance::dto::receivable::InvoiceRequestQuery,
        actor: &AuditActor,
    ) -> Result<FundsScopedPage<ScopedInvoiceRequestRow>> {
        query.validate_scope_filters().map_err(Error::from)?;
        let page = query.page.unwrap_or(1).max(1);
        ensure_page(page, query.scope_version.as_deref())?;
        let snapshot = self.checked_requests(query, actor, query.scope_version.as_deref()).await?;
        Ok(snapshot)
    }

    /// 独立详情重新解析开票申请详情动作；不可见与不存在统一为 NotFound。
    pub async fn request_detail_scoped(
        &self,
        id: &str,
        actor: &AuditActor,
    ) -> Result<FundsScopedResult<ScopedInvoiceRequestRow>> {
        let this = self.clone();
        let actor = actor.clone();
        let id = id.to_string();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move { this.load_request_detail(&id, &actor, executor).await })
            })
            .await
    }

    /// 返回前重读授权及候选事实版本；变化时拒绝交付原结果。
    pub(super) async fn checked_requests(
        &self,
        query: &erp_finance::dto::receivable::InvoiceRequestQuery,
        actor: &AuditActor,
        expected: Option<&str>,
    ) -> Result<FundsScopedPage<ScopedInvoiceRequestRow>> {
        checked_twice(expected, || self.snapshot_requests(query, actor)).await
    }

    /// 身份和业务事实均使用调用方同一事务，不缓存权限解析结果。
    pub(super) async fn snapshot_requests(
        &self,
        query: &erp_finance::dto::receivable::InvoiceRequestQuery,
        actor: &AuditActor,
    ) -> Result<FundsScopedPage<ScopedInvoiceRequestRow>> {
        let this = self.clone();
        let query = query.clone();
        let actor = actor.clone();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move { this.load_requests(&query, &actor, executor).await })
            })
            .await
    }

    /// 分页查询开票申请范围行：申请人取创建人，处理人取工作项当前负责人。
    pub(super) async fn load_requests(
        &self,
        query: &erp_finance::dto::receivable::InvoiceRequestQuery,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<FundsScopedPage<ScopedInvoiceRequestRow>> {
        let (access, authorization) = self.resolve(actor, "sales_invoice_request", "list", executor).await?;
        if authorization.empty() {
            return Ok(empty_page(&authorization, "no_scope", "开票申请无可见范围"));
        }
        let candidates = self.page_all_requests(query, executor).await?;
        let RequestSnapshot { decided, facts, order_nos } =
            self.assemble_requests(query, candidates, &access, &authorization, executor).await?;
        let mut fingerprint = std::collections::hash_map::DefaultHasher::new();
        authorization.context.scope_version.hash(&mut fingerprint);
        for (row, _) in decided.iter() {
            row.base.id.hash(&mut fingerprint);
            row.base.version.hash(&mut fingerprint);
            if let Some(order) = facts.get(&row.sales_order_id.to_string()) {
                order.version.hash(&mut fingerprint);
            }
        }
        self.finish_requests(query, decided, facts, order_nos, authorization, fingerprint, executor).await
    }

    /// 有界完整装载候选；超过既有上限整体拒绝，不改变最终授权裁剪与分页。
    pub(super) async fn page_all_requests(
        &self,
        query: &erp_finance::dto::receivable::InvoiceRequestQuery,
        executor: &mut dyn Executor,
    ) -> Result<Vec<SalesInvoiceRequest>> {
        bounded_request_candidates(self.db.sales_invoice_requests().scope_candidates(query, executor).await?)
    }

    /// 开票申请候选逐行判定可见性与筛选；缺失销售单的行跳过，不计未分配。
    pub(super) async fn assemble_requests(
        &self,
        query: &erp_finance::dto::receivable::InvoiceRequestQuery,
        rows: Vec<SalesInvoiceRequest>,
        access: &FundsResolvedScope,
        authorization: &FundsAuthorization,
        executor: &mut dyn Executor,
    ) -> Result<RequestSnapshot> {
        let order_ids = rows.iter().map(|row| row.sales_order_id.to_string()).collect::<Vec<_>>();
        let (facts, order_nos) = self.request_sales_context(&order_ids, executor).await?;
        let authorized = self.authorized_sales_ids(authorization, executor).await?;
        let allowed = authorized.map(|list| list.into_iter().collect::<BTreeSet<_>>());
        let work_ids = rows.iter().filter_map(|row| row.work_item_id.clone()).collect::<Vec<_>>();
        let handlers = self.work_item_handlers(&work_ids, executor).await?;
        let condition = self.request_condition(query, executor).await?;
        let decided = decide_requests(rows, &facts, &allowed, &handlers, access, &condition)?;
        Ok(RequestSnapshot { decided, facts, order_nos })
    }

    /// 开票申请关联筛选条件：负责人、申请人、处理人与组织分别精确匹配。
    pub(super) async fn request_condition(
        &self,
        query: &erp_finance::dto::receivable::InvoiceRequestQuery,
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
            operator_user_ids: query.applicant_user_ids.as_ref().map(|ids| ids.as_slice().to_vec()),
            secondary_operator_user_ids: query.handler_user_ids.as_ref().map(|ids| ids.as_slice().to_vec()),
            org_unit_ids,
        })
    }

    /// 本次快照一次读取销售来源，同时供授权、版本及单号展示使用。
    async fn request_sales_context(
        &self,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<(HashMap<String, LinkedSalesFact>, HashMap<String, String>)> {
        let mut facts = HashMap::new();
        let mut numbers = HashMap::new();
        let unique = crate::support::dedup_sorted(ids.iter().cloned());
        for chunk in unique.chunks(500) {
            let keys = chunk.iter().map(SalesOrderId::new).collect::<Vec<_>>();
            for order in self.db.sales_orders().find_orders_by_ids(&keys, executor).await? {
                numbers.insert(order.base.id.clone(), order.order_no);
                facts.insert(
                    order.base.id,
                    LinkedSalesFact {
                        owner_user_id: order.sales_owner_user_id,
                        business_org_unit_id: order.business_org_unit_id,
                        version: order.base.version,
                    },
                );
            }
        }
        Ok((facts, numbers))
    }

    /// 开票申请候选分页裁剪与汇总装配；申请金额为行级事实，始终返回。
    // 查询+分页+执行器参数为既有签名，保持调用方一致不拆。
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn finish_requests(
        &self,
        query: &erp_finance::dto::receivable::InvoiceRequestQuery,
        decided: Vec<(SalesInvoiceRequest, Option<String>)>,
        facts: HashMap<String, LinkedSalesFact>,
        order_nos: HashMap<String, String>,
        authorization: FundsAuthorization,
        fingerprint: std::collections::hash_map::DefaultHasher,
        _executor: &mut dyn Executor,
    ) -> Result<FundsScopedPage<ScopedInvoiceRequestRow>> {
        let total = decided.len() as u64;
        let page = query.page.unwrap_or(1).max(1);
        let page_size = query.page_size.unwrap_or(20).clamp(1, 100);
        let start = ((page - 1) as usize).saturating_mul(page_size as usize);
        let end = start.saturating_add(page_size as usize).min(decided.len());
        let whole = true;
        let items = decided
            .get(start..end)
            .unwrap_or_default()
            .iter()
            .map(|(row, handler)| request_row(row, handler.clone(), &facts, &order_nos))
            .collect();
        let triples = decided
            .iter()
            .map(|(row, _)| (row.base.id.clone(), row.data.amount, Some(row.sales_order_id.to_string())))
            .collect::<Vec<_>>();
        let owner_of = facts
            .iter()
            .map(|(id, fact)| (id.clone(), fact.owner_user_id.clone()))
            .collect::<HashMap<_, _>>();
        let version = format!("{:x}", fingerprint.finish());
        ensure_version(query.scope_version.as_deref(), &version).map_err(|_| changed())?;
        let summary = build_summary(&triples, &owner_of, None, &version, !whole)?;
        Ok(FundsScopedPage {
            items,
            total,
            summary,
            page,
            page_size,
            scope_version: version.clone(),
            policy_version: authorization.context.policy_version,
            organization_version: authorization.context.organizations.version,
            as_of: authorization.context.as_of.as_utc().to_rfc3339(),
            empty_reason: None,
            scope_summary: "开票申请按关联销售当前负责人、申请人与当前开票处理人授权；不改变正式开票准入",
            ownership_basis: "linked_sales_owner_applicant_and_handler",
        })
    }

    /// 开票申请详情同一事务内解析、取数与裁剪；版本绑定关联销售单。
    pub(super) async fn load_request_detail(
        &self,
        id: &str,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<FundsScopedResult<ScopedInvoiceRequestRow>> {
        let (access, authorization) =
            self.resolve(actor, "sales_invoice_request", "detail", executor).await?;
        let row = self
            .db
            .sales_invoice_requests()
            .find_by_id(id, executor)
            .await
            .map_err(Error::from)?
            .ok_or_else(|| Error::NotFound("开票申请不存在".into()))?;
        let order_ids = vec![row.sales_order_id.to_string()];
        let (facts, order_nos) = self.request_sales_context(&order_ids, executor).await?;
        let fact = facts
            .get(&row.sales_order_id.to_string())
            .ok_or_else(|| Error::NotFound("开票申请不存在".into()))?;
        let authorized = self.authorized_sales_ids(&authorization, executor).await?;
        if let Some(allowed) = &authorized
            && !allowed.contains(&row.sales_order_id.to_string())
        {
            return Err(Error::NotFound("开票申请不存在".into()));
        }
        let handlers = self
            .work_item_handlers(&row.work_item_id.clone().into_iter().collect::<Vec<_>>(), executor)
            .await?;
        let handler = row.work_item_id.as_ref().and_then(|item| handlers.get(item).cloned());
        let row_facts = request_linked_facts(&row, fact, handler.as_deref());
        if !Self::allows(&access, &row_facts)? {
            return Err(Error::NotFound("开票申请不存在".into()));
        }
        let data = request_row(&row, handler, &facts, &order_nos);
        let parts = vec![format!("{}:{}", row.base.id, row.base.version), row_facts.version_part()];
        Ok(FundsScopedResult {
            data,
            scope_version: scope_version(&authorization.context, &parts),
            policy_version: authorization.context.policy_version,
            organization_version: authorization.context.organizations.version,
            as_of: authorization.context.as_of.as_utc().to_rfc3339(),
            empty_reason: None,
            scope_summary: "开票申请按关联销售当前负责人、申请人与当前开票处理人授权；不改变正式开票准入",
            ownership_basis: "linked_sales_owner_applicant_and_handler",
        })
    }
}

/// 本次申请快照的已决行与来源事实，禁止跨独立快照共享。
pub(super) struct RequestSnapshot {
    decided: Vec<(SalesInvoiceRequest, Option<String>)>,
    facts: HashMap<String, LinkedSalesFact>,
    order_nos: HashMap<String, String>,
}

/// 完整候选达到 10001 条时整体拒绝，恰好 10000 条仍完整返回。
fn bounded_request_candidates(rows: Vec<SalesInvoiceRequest>) -> Result<Vec<SalesInvoiceRequest>> {
    if rows.len() > 10_000 {
        return Err(Error::ValidationError("开票申请查询超过上限，请收窄组织或负责人条件".into()));
    }
    Ok(rows)
}

/// 按原候选顺序裁剪来源授权与业务条件，来源缺失仍跳过，不推导未分配份额。
fn decide_requests(
    rows: Vec<SalesInvoiceRequest>,
    facts: &HashMap<String, LinkedSalesFact>,
    allowed: &Option<BTreeSet<String>>,
    handlers: &HashMap<String, String>,
    access: &FundsResolvedScope,
    condition: &FundsLinkedCondition,
) -> Result<Vec<(SalesInvoiceRequest, Option<String>)>> {
    let mut decided = Vec::new();
    for row in rows {
        let Some(fact) = facts.get(row.sales_order_id.as_ref()) else {
            continue;
        };
        if allowed.as_ref().is_some_and(|ids| !ids.contains(row.sales_order_id.as_ref())) {
            continue;
        }
        let handler = row.work_item_id.as_ref().and_then(|id| handlers.get(id).cloned());
        let row_facts = request_linked_facts(&row, fact, handler.as_deref());
        if FundsAccess::allows(access, &row_facts)? && matches_linked_condition(&row_facts, condition) {
            decided.push((row, handler));
        }
    }
    Ok(decided)
}

/// 来源责任、申请人和工作项处理人沿用各自事实口径。
fn request_linked_facts(
    row: &SalesInvoiceRequest,
    fact: &LinkedSalesFact,
    handler: Option<&str>,
) -> FundsLinkedFacts {
    FundsLinkedFacts {
        owner_user_id: Some(fact.owner_user_id.clone()),
        business_org_unit_id: Some(fact.business_org_unit_id.clone()),
        operator_user_ids: vec![row.created_by.clone()],
        secondary_operator_user_ids: handler.map(str::to_string).into_iter().collect(),
        linked_document_id: row.sales_order_id.to_string(),
        linked_document_version: fact.version,
    }
}

/// 按同拍已读取的来源事实生成范围行，申请金额始终作为行级金额返回。
fn request_row(
    row: &SalesInvoiceRequest,
    handler: Option<String>,
    facts: &HashMap<String, LinkedSalesFact>,
    order_nos: &HashMap<String, String>,
) -> ScopedInvoiceRequestRow {
    let fact = facts.get(row.sales_order_id.as_ref());
    ScopedInvoiceRequestRow {
        id: row.base.id.clone(),
        request_no: row.request_no.clone(),
        sales_order_id: row.sales_order_id.to_string(),
        sales_order_no: order_nos.get(row.sales_order_id.as_ref()).cloned().unwrap_or_default(),
        status: row.status,
        created_at: row.base.created_at,
        applicant_user_id: row.created_by.clone(),
        handler_user_id: handler,
        amount: row.data.amount,
        permission_limited: false,
        sales_owner_user_id: fact.map(|order| order.owner_user_id.clone()),
        business_org_unit_id: fact.map(|order| order.business_org_unit_id.clone()),
    }
}

#[cfg(test)]
mod tests {
    use entity_core::BaseModel;
    use erp_core::common::time::Instant;
    use erp_core::ids::{CustomerAccountId, PartyId, ReceivableAccountId};
    use erp_core::money::Amount;
    use erp_finance::entity::receivable::{InvoiceRequestData, InvoiceRequestStatus};
    use erp_finance::ports::funds_scope::FundsResolvedClause;

    use super::*;

    /// 构造来自仓储的开票申请候选。
    fn request(id: &str, order: &str, work: Option<&str>) -> SalesInvoiceRequest {
        SalesInvoiceRequest {
            base: BaseModel { id: id.into(), version: 4, ..BaseModel::fake() },
            request_no: format!("KP-{id}"),
            receivable_account_id: ReceivableAccountId::new("account"),
            sales_order_id: SalesOrderId::new(order),
            customer_id: CustomerAccountId::new("customer"),
            counterparty_party_id: PartyId::new("party"),
            created_by: "applicant".into(),
            status: InvoiceRequestStatus::Approved,
            approval_subject_version: 1,
            data: InvoiceRequestData {
                amount: "9.99".parse().unwrap(),
                invoice_title: "title".into(),
                tax_number: "tax".into(),
                invoice_content: "content".into(),
                reason: "reason".into(),
            },
            invoiced_amount: Amount::zero(),
            work_item_id: work.map(str::to_string),
        }
    }

    /// 构造仅在本次查询中使用的公司范围，个人上限缺省。
    fn access() -> FundsResolvedScope {
        FundsResolvedScope {
            user_id: "reader".into(),
            resource: "sales_invoice_request".into(),
            action: "list".into(),
            role_clauses: vec![FundsResolvedClause { company: true, ..Default::default() }],
            user_limit: None,
            policy_version: 1,
            organization_version: 1,
            scope_version: "v".into(),
            as_of: Instant::from_unix_secs(1),
        }
    }

    /// 构造不附加业务条件的范围筛选。
    fn condition() -> FundsLinkedCondition {
        FundsLinkedCondition {
            owner_user_ids: None,
            operator_user_ids: None,
            secondary_operator_user_ids: None,
            org_unit_ids: None,
        }
    }

    /// 构造同拍来源责任和业务版本。
    fn fact(owner: &str, org: &str, version: u64) -> LinkedSalesFact {
        LinkedSalesFact { owner_user_id: owner.into(), business_org_unit_id: org.into(), version }
    }

    /// 空候选及恰好上限完整交付，哨兵触发既有整体拒绝和原文案。
    #[test]
    fn scoped_request_candidates_keep_exact_limit_and_reject_overflow() {
        assert!(bounded_request_candidates(Vec::new()).unwrap().is_empty());
        let row = request("r", "so", None);
        let allowed = bounded_request_candidates(vec![row.clone(); 10_000]).unwrap();
        assert_eq!(allowed.len(), 10_000);
        let error = bounded_request_candidates(vec![row; 10_001]).unwrap_err();
        assert!(matches!(
            error,
            Error::ValidationError(message) if message == "开票申请查询超过上限，请收窄组织或负责人条件"
        ));
    }

    /// 共享来源上下文保留候选顺序、缺来源跳过、无任务处理人及展示字段。
    #[test]
    fn scoped_request_snapshot_reuses_source_facts_and_keeps_order_missing_and_handler_behavior() {
        let rows = vec![
            request("second", "so-a", Some("work")),
            request("missing", "deleted", Some("work")),
            request("first", "so-b", Some("missing-work")),
        ];
        let facts = HashMap::from([
            ("so-a".into(), fact("owner-a", "org-a", 10)),
            ("so-b".into(), fact("owner-b", "org-b", 20)),
        ]);
        let numbers = HashMap::from([("so-a".into(), "SO-A".into()), ("so-b".into(), "SO-B".into())]);
        let handlers = HashMap::from([("work".into(), "handler".into())]);
        let decided = decide_requests(rows, &facts, &None, &handlers, &access(), &condition()).unwrap();
        assert_eq!(
            decided.iter().map(|(row, _)| row.base.id.as_str()).collect::<Vec<_>>(),
            ["second", "first"]
        );
        assert_eq!(decided[0].1.as_deref(), Some("handler"));
        assert_eq!(decided[1].1, None);
        let row = request_row(&decided[0].0, decided[0].1.clone(), &facts, &numbers);
        assert_eq!(row.sales_order_no, "SO-A");
        assert_eq!(row.sales_owner_user_id.as_deref(), Some("owner-a"));
        assert_eq!(row.business_org_unit_id.as_deref(), Some("org-a"));
        assert_eq!(row.applicant_user_id, "applicant");
        assert_eq!(row.handler_user_id.as_deref(), Some("handler"));
        assert_eq!(row.amount, "9.99".parse().unwrap());
        assert!(!row.permission_limited);
        let linked = request_linked_facts(&decided[0].0, &facts["so-a"], decided[0].1.as_deref());
        assert_eq!(linked.linked_document_version, 10);
        assert_eq!(linked.linked_document_id, "so-a");
    }

    /// 来源授权与所有业务条件取交集；权限政策不因共享事实变宽。
    #[test]
    fn scoped_request_snapshot_keeps_authorization_and_all_filter_intersections() {
        let rows = vec![request("a", "so-a", Some("work")), request("b", "so-b", None)];
        let facts = HashMap::from([
            ("so-a".into(), fact("owner-a", "org-a", 1)),
            ("so-b".into(), fact("owner-b", "org-b", 2)),
        ]);
        let handlers = HashMap::from([("work".into(), "handler".into())]);
        let mut filters = condition();
        filters.owner_user_ids = Some(vec!["owner-a".into()]);
        filters.operator_user_ids = Some(vec!["applicant".into()]);
        filters.secondary_operator_user_ids = Some(vec!["handler".into()]);
        filters.org_unit_ids = Some(vec!["org-a".into()]);
        let allowed = Some(BTreeSet::from(["so-a".into()]));
        let decided =
            decide_requests(rows.clone(), &facts, &allowed, &handlers, &access(), &filters).unwrap();
        assert_eq!(decided.len(), 1);
        assert_eq!(decided[0].0.base.id, "a");
        filters.org_unit_ids = Some(vec!["org-b".into()]);
        assert!(
            decide_requests(rows.clone(), &facts, &allowed, &handlers, &access(), &filters)
                .unwrap()
                .is_empty()
        );
        assert!(
            decide_requests(rows.clone(), &facts, &Some(BTreeSet::new()), &handlers, &access(), &condition())
                .unwrap()
                .is_empty()
        );
        let mut access = access();
        access.user_limit = Some(FundsResolvedClause::default());
        assert!(decide_requests(rows, &facts, &None, &handlers, &access, &condition()).unwrap().is_empty());
    }
}
