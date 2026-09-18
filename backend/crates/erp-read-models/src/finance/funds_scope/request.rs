//! 开票申请范围查询。

use std::collections::{BTreeSet, HashMap};
use std::hash::{Hash, Hasher};

use application_core::AuditActor;
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
        let decided = self.assemble_requests(query, candidates, &access, &authorization, executor).await?;
        let order_ids = decided.iter().map(|(row, _)| row.sales_order_id.to_string()).collect::<Vec<_>>();
        let facts = self.sales_fact_map(&order_ids, executor).await?;
        let order_nos = self.sales_order_nos(&order_ids, executor).await?;
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

    /// 申请仓储按百行分页全量取回候选；超过上限整体拒绝，不得截断。
    pub(super) async fn page_all_requests(
        &self,
        query: &erp_finance::dto::receivable::InvoiceRequestQuery,
        executor: &mut dyn Executor,
    ) -> Result<Vec<SalesInvoiceRequest>> {
        let mut items = Vec::new();
        let mut page_no = 1u64;
        loop {
            let mut paged = query.clone();
            paged.page = Some(page_no);
            paged.page_size = Some(100);
            let result = self.db.sales_invoice_requests().page(&paged, executor).await?;
            if result.total > 10_000 {
                return Err(Error::ValidationError("开票申请查询超过上限，请收窄组织或负责人条件".into()));
            }
            let done = result.items.len() < 100;
            items.extend(result.items);
            if done || items.len() as i64 >= result.total {
                break;
            }
            page_no += 1;
        }
        Ok(items)
    }

    /// 开票申请候选逐行判定可见性与筛选；缺失销售单的行跳过，不计未分配。
    pub(super) async fn assemble_requests(
        &self,
        query: &erp_finance::dto::receivable::InvoiceRequestQuery,
        rows: Vec<SalesInvoiceRequest>,
        access: &FundsResolvedScope,
        authorization: &FundsAuthorization,
        executor: &mut dyn Executor,
    ) -> Result<Vec<(SalesInvoiceRequest, Option<String>)>> {
        let order_ids = rows.iter().map(|row| row.sales_order_id.to_string()).collect::<Vec<_>>();
        let facts = self.sales_fact_map(&order_ids, executor).await?;
        let authorized = self.authorized_sales_ids(authorization, executor).await?;
        let allowed = authorized.map(|list| list.into_iter().collect::<BTreeSet<_>>());
        let work_ids = rows.iter().filter_map(|row| row.work_item_id.clone()).collect::<Vec<_>>();
        let handlers = self.work_item_handlers(&work_ids, executor).await?;
        let condition = self.request_condition(query, executor).await?;
        let mut decided = Vec::new();
        for row in rows {
            let Some(fact) = facts.get(&row.sales_order_id.to_string()) else {
                continue;
            };
            if let Some(allowed) = &allowed
                && !allowed.contains(&row.sales_order_id.to_string())
            {
                continue;
            }
            let handler = row.work_item_id.as_ref().and_then(|id| handlers.get(id).cloned());
            let row_facts = FundsLinkedFacts {
                owner_user_id: Some(fact.owner_user_id.clone()),
                business_org_unit_id: Some(fact.business_org_unit_id.clone()),
                operator_user_ids: vec![row.created_by.clone()],
                secondary_operator_user_ids: handler.clone().into_iter().collect(),
                linked_document_id: row.sales_order_id.to_string(),
                linked_document_version: fact.version,
            };
            if !Self::allows(access, &row_facts)? {
                continue;
            }
            if !matches_linked_condition(&row_facts, &condition) {
                continue;
            }
            decided.push((row, handler));
        }
        Ok(decided)
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

    /// 关联销售单号一次取回；缺失单据的行已在装载阶段跳过。
    pub(super) async fn sales_order_nos(
        &self,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, String>> {
        use erp_core::ids::SalesOrderId;
        let mut map = HashMap::new();
        let unique = crate::support::dedup_sorted(ids.iter().cloned());
        for chunk in unique.chunks(500) {
            let keys = chunk.iter().map(|id| SalesOrderId::new(id.clone())).collect::<Vec<_>>();
            for order in self.db.sales_orders().find_orders_by_ids(&keys, executor).await? {
                map.insert(order.base.id.clone(), order.order_no.clone());
            }
        }
        Ok(map)
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
        executor: &mut dyn Executor,
    ) -> Result<FundsScopedPage<ScopedInvoiceRequestRow>> {
        let total = decided.len() as u64;
        let page = query.page.unwrap_or(1).max(1);
        let page_size = query.page_size.unwrap_or(20).clamp(1, 100);
        let start = ((page - 1) as usize).saturating_mul(page_size as usize);
        let end = start.saturating_add(page_size as usize).min(decided.len());
        let whole = authorization.whole();
        let mut items = Vec::new();
        if start < decided.len() {
            for (row, handler) in decided[start..end].iter() {
                let fact = facts.get(&row.sales_order_id.to_string());
                items.push(ScopedInvoiceRequestRow {
                    id: row.base.id.clone(),
                    request_no: row.request_no.clone(),
                    sales_order_id: row.sales_order_id.to_string(),
                    sales_order_no: order_nos
                        .get(&row.sales_order_id.to_string())
                        .cloned()
                        .unwrap_or_default(),
                    status: row.status,
                    created_at: row.base.created_at,
                    applicant_user_id: row.created_by.clone(),
                    handler_user_id: handler.clone(),
                    amount: row.data.amount,
                    permission_limited: !whole,
                    sales_owner_user_id: fact.map(|order| order.owner_user_id.clone()),
                    business_org_unit_id: fact.map(|order| order.business_org_unit_id.clone()),
                });
            }
        }
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
        let facts = self.sales_fact_map(&order_ids, executor).await?;
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
        let row_facts = FundsLinkedFacts {
            owner_user_id: Some(fact.owner_user_id.clone()),
            business_org_unit_id: Some(fact.business_org_unit_id.clone()),
            operator_user_ids: vec![row.created_by.clone()],
            secondary_operator_user_ids: handler.clone().into_iter().collect(),
            linked_document_id: row.sales_order_id.to_string(),
            linked_document_version: fact.version,
        };
        if !Self::allows(&access, &row_facts)? {
            return Err(Error::NotFound("开票申请不存在".into()));
        }
        let order_nos = self.sales_order_nos(&order_ids, executor).await?;
        let whole = authorization.whole();
        let data = ScopedInvoiceRequestRow {
            id: row.base.id.clone(),
            request_no: row.request_no.clone(),
            sales_order_id: row.sales_order_id.to_string(),
            sales_order_no: order_nos.get(&row.sales_order_id.to_string()).cloned().unwrap_or_default(),
            status: row.status,
            created_at: row.base.created_at,
            applicant_user_id: row.created_by.clone(),
            handler_user_id: handler,
            amount: row.data.amount,
            permission_limited: !whole,
            sales_owner_user_id: Some(fact.owner_user_id.clone()),
            business_org_unit_id: Some(fact.business_org_unit_id.clone()),
        };
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
