//! 开票申请范围查询。

use std::collections::HashMap;
use std::hash::{Hash, Hasher};

use application_core::AuditActor;
use erp_core::ids::SalesOrderId;
use erp_finance::entity::receivable::SalesInvoiceRequest;
use erp_finance::repository::ReceivableExt;
use erp_finance::repository::prelude::*;
use erp_sales::repository::SalesOrderExt;
use erp_sales::repository::prelude::*;
use erp_workflow::ErrorCode;
use persistence_core::{Executor, Transactional};

use super::authorization::{FundsReadContext, *};
use super::linked::organization_condition;
use super::rows::*;
use crate::{Error, Result};

impl FundsAccess {
    /// 分页查询开票申请范围行：负责销售/申请人/当前开票处理人分别查询。
    ///
    /// # 参数
    /// * `query` - 开票申请列表查询。
    /// * `actor` - 已认证操作人。
    ///
    /// # 返回
    /// 最终授权申请页及同口径汇总。
    ///
    /// # 错误
    /// 筛选校验、范围版本、授权或读取失败时返回对应错误。
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
    ///
    /// # 参数
    /// * `id` - 开票申请主键。
    /// * `actor` - 已认证操作人。
    ///
    /// # 返回
    /// 当前详情动作允许的申请行。
    ///
    /// # 错误
    /// 授权或读取失败时返回对应错误；申请不存在、来源销售单缺失或不在授权内时返回 `NotFound`。
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
    ///
    /// # 参数
    /// * `query` - 开票申请列表查询。
    /// * `actor` - 已认证操作人。
    /// * `expected` - 调用方回传的范围版本；首页可为 `None`。
    ///
    /// # 返回
    /// 两次快照版本一致时返回第一次申请页。
    ///
    /// # 错误
    /// 任一拍版本失配时返回范围变化冲突错误；授权或读取失败时返回对应错误。
    pub(super) async fn checked_requests(
        &self,
        query: &erp_finance::dto::receivable::InvoiceRequestQuery,
        actor: &AuditActor,
        expected: Option<&str>,
    ) -> Result<FundsScopedPage<ScopedInvoiceRequestRow>> {
        checked_revalidated(
            expected,
            || self.snapshot_requests(query, actor),
            || self.request_fingerprint_snapshot(query, actor),
        )
        .await
    }

    /// 身份和业务事实均使用调用方同一事务，不缓存权限解析结果。
    ///
    /// # 参数
    /// * `query` - 开票申请列表查询。
    /// * `actor` - 已认证操作人。
    ///
    /// # 返回
    /// 同一事务内形成的授权申请页。
    ///
    /// # 错误
    /// 事务、授权或读取失败时返回对应错误。
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

    /// 分页查询开票申请范围行：申请人取创建人，处理人优先工作项负责人、否则完成人。
    ///
    /// # 参数
    /// * `query` - 开票申请列表查询。
    /// * `actor` - 已认证操作人。
    /// * `executor` - 调用方事务。
    ///
    /// # 返回
    /// 无可见范围时返回空页；否则返回裁剪后的申请页与汇总。
    ///
    /// # 错误
    /// 授权、候选读取、来源事实或版本不一致时返回对应错误。
    pub(super) async fn load_requests(
        &self,
        query: &erp_finance::dto::receivable::InvoiceRequestQuery,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<FundsScopedPage<ScopedInvoiceRequestRow>> {
        let (_, authorization) = self.resolve(actor, "sales_invoice_request", "list", executor).await?;
        if authorization.empty() {
            return Ok(empty_page(&authorization, "no_scope", "开票申请无可见范围"));
        }
        if let Some(task_id) = &query.work_item_id
            && let Some(request) = self.db.sales_invoice_requests().find_for_task(task_id, executor).await?
        {
            self.ensure_request_ready(&request, actor, &authorization, executor).await?;
        }
        let candidates = self.request_candidates(query, &authorization, executor).await?;
        let snapshot = self.assemble_requests(candidates, executor).await?;
        let version = request_version(
            &authorization.context,
            snapshot.decided.iter().map(|(row, _)| row),
            &snapshot.facts,
        );
        finish_requests(query, snapshot, authorization, version)
    }

    /// 仅水合已经通过数据库授权和业务筛选的申请，不再解释范围或重做条件裁剪。
    ///
    /// # 参数
    /// * `rows` - 数据库返回的完整授权候选。
    /// * `executor` - 原读取事务。
    ///
    /// # 返回
    /// 候选顺序不变的申请、展示处理人及销售责任事实。
    ///
    /// # 错误
    /// 销售来源或工作项读取失败时返回原错误。
    async fn assemble_requests(
        &self,
        rows: Vec<SalesInvoiceRequest>,
        executor: &mut dyn Executor,
    ) -> Result<RequestSnapshot> {
        let order_ids = rows.iter().map(|row| row.sales_order_id.to_string()).collect::<Vec<_>>();
        let (facts, order_nos) = self.request_sales_context(&order_ids, executor).await?;
        let work_ids = rows.iter().filter_map(|row| row.work_item_id.clone()).collect::<Vec<_>>();
        let handlers = self.work_item_handlers(&work_ids, executor).await?;
        let decided = hydrate_requests(rows, &facts, &handlers);
        Ok(RequestSnapshot { decided, facts, order_nos })
    }

    /// 独立复核完整候选及销售版本，不重读展示处理人、不构造页面和汇总。
    async fn request_fingerprint_snapshot(
        &self,
        query: &erp_finance::dto::receivable::InvoiceRequestQuery,
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
                        this.resolve(&actor, "sales_invoice_request", "list", executor).await?;
                    if authorization.empty() {
                        return Ok(authorization.context.scope_version);
                    }
                    let rows = this.request_candidates(&query, &authorization, executor).await?;
                    let order_ids = rows.iter().map(|row| row.sales_order_id.to_string()).collect::<Vec<_>>();
                    let (facts, _) = this.request_sales_context(&order_ids, executor).await?;
                    Ok(request_version(&authorization.context, rows.iter(), &facts))
                })
            })
            .await
    }

    /// 开票申请关联筛选条件：负责人、申请人、处理人与组织分别精确匹配。
    ///
    /// # 参数
    /// * `query` - 开票申请列表查询。
    /// * `executor` - 调用方事务。
    ///
    /// # 返回
    /// 销售负责人、申请人、处理人与展开组织的精确条件。
    ///
    /// # 错误
    /// 组织展开失败时返回对应错误。
    pub(super) async fn request_condition(
        &self,
        query: &erp_finance::dto::receivable::InvoiceRequestQuery,
        executor: &mut dyn Executor,
    ) -> Result<FundsLinkedCondition> {
        let org_unit_ids = organization_condition(
            self.scope.as_ref(),
            query.org_unit_ids.as_ref(),
            query.include_descendants,
            executor,
        )
        .await?;
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

    /// 开票申请详情同一事务内解析、取数与裁剪；版本绑定关联销售单。
    ///
    /// # 参数
    /// * `id` - 开票申请主键。
    /// * `actor` - 已认证操作人。
    /// * `executor` - 调用方事务。
    ///
    /// # 返回
    /// 详情动作允许的申请行及范围版本。
    ///
    /// # 错误
    /// 授权、读取或未接线的资源动作失败时返回对应错误；申请不存在、来源销售单缺失、不在销售单集合内或单对象判定不允许时返回 `NotFound`。
    pub(super) async fn load_request_detail(
        &self,
        id: &str,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<FundsScopedResult<ScopedInvoiceRequestRow>> {
        let (_access, authorization) =
            self.resolve(actor, "sales_invoice_request", "detail", executor).await?;
        let row = self
            .db
            .sales_invoice_requests()
            .find_by_id(id, executor)
            .await
            .map_err(Error::from)?
            .ok_or_else(|| Error::NotFound("开票申请不存在".into()))?;
        self.ensure_request_ready(&row, actor, &authorization, executor).await?;
        let order_ids = vec![row.sales_order_id.to_string()];
        let (facts, order_nos) = self.request_sales_context(&order_ids, executor).await?;
        let fact = facts
            .get(&row.sales_order_id.to_string())
            .ok_or_else(|| Error::NotFound("开票申请不存在".into()))?;
        let authorized = self.authorized_sales_targets(&authorization, &order_ids, executor).await?;
        if !authorized.contains(&row.sales_order_id.to_string()) {
            return Err(Error::NotFound("开票申请不存在".into()));
        }
        let handlers = self
            .work_item_handlers(&row.work_item_id.clone().into_iter().collect::<Vec<_>>(), executor)
            .await?;
        let handler = row.work_item_id.as_ref().and_then(|item| handlers.get(item).cloned());
        let row_facts = request_linked_facts(&row, fact, handler.as_deref());
        let data = request_row(&row, handler, &facts, &order_nos);
        let parts = vec![format!("{}:{}", row.base.id, row.base.version), row_facts.version_part()];
        Ok(FundsScopedResult {
            data,
            scope_version: scope_version(&authorization.context, &parts),
            as_of: authorization.context.as_of.as_utc().to_rfc3339(),
            empty_reason: None,
            scope_summary: "开票申请按关联销售当前负责人、申请人与当前开票处理人授权；不改变正式开票准入",
            ownership_basis: "linked_sales_owner_applicant_and_handler",
        })
    }
}

/// 只对已经确定的完整候选分页并汇总，不执行授权或数据库读取。
fn finish_requests(
    query: &erp_finance::dto::receivable::InvoiceRequestQuery,
    snapshot: RequestSnapshot,
    authorization: FundsAuthorization,
    version: String,
) -> Result<FundsScopedPage<ScopedInvoiceRequestRow>> {
    let RequestSnapshot { decided, facts, order_nos } = snapshot;
    let total = decided.len() as u64;
    let page = query.page.unwrap_or(1).max(1);
    let page_size = query.page_size.unwrap_or(20).clamp(1, 100);
    let start = ((page - 1) as usize).saturating_mul(page_size as usize);
    let end = start.saturating_add(page_size as usize).min(decided.len());
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
    let owner_of =
        facts.iter().map(|(id, fact)| (id.clone(), fact.owner_user_id.clone())).collect::<HashMap<_, _>>();
    ensure_version(query.scope_version.as_deref(), &version).map_err(|_| changed())?;
    let summary = build_summary(&triples, &owner_of, None, &version, false)?;
    Ok(FundsScopedPage {
        items,
        total,
        summary,
        page,
        page_size,
        scope_version: version.clone(),
        as_of: authorization.context.as_of.as_utc().to_rfc3339(),
        empty_reason: None,
        scope_summary: "开票申请按关联销售当前负责人、申请人与当前开票处理人授权；不改变正式开票准入",
        ownership_basis: "linked_sales_owner_applicant_and_handler",
    })
}

/// 范围及完整匹配集合的业务版本共用同一编码，复核拍不依赖展示字段。
fn request_version<'a>(
    context: &FundsReadContext,
    rows: impl Iterator<Item = &'a SalesInvoiceRequest>,
    facts: &HashMap<String, LinkedSalesFact>,
) -> String {
    let mut fingerprint = std::collections::hash_map::DefaultHasher::new();
    context.scope_version.hash(&mut fingerprint);
    for row in rows {
        let Some(order) = facts.get(row.sales_order_id.as_ref()) else { continue };
        row.base.id.hash(&mut fingerprint);
        row.base.version.hash(&mut fingerprint);
        order.version.hash(&mut fingerprint);
    }
    format!("{:x}", fingerprint.finish())
}

/// 本次申请快照的已决行与来源事实，禁止跨独立快照共享。
struct RequestSnapshot {
    decided: Vec<(SalesInvoiceRequest, Option<String>)>,
    facts: HashMap<String, LinkedSalesFact>,
    order_nos: HashMap<String, String>,
}

/// 完整候选达到 10001 条时整体拒绝，恰好 10000 条仍完整返回。
pub(super) fn bounded_request_candidates(rows: Vec<SalesInvoiceRequest>) -> Result<Vec<SalesInvoiceRequest>> {
    if rows.len() > 10_000 {
        return Err(Error::ValidationError("开票申请查询超过上限，请收窄组织或负责人条件".into()));
    }
    Ok(rows)
}

/// 原候选已在数据库完成授权和筛选，仅补展示处理人，缺失来源沿既有合同跳过。
fn hydrate_requests(
    rows: Vec<SalesInvoiceRequest>,
    facts: &HashMap<String, LinkedSalesFact>,
    handlers: &HashMap<String, String>,
) -> Vec<(SalesInvoiceRequest, Option<String>)> {
    rows.into_iter()
        .filter_map(|row| {
            facts.get(row.sales_order_id.as_ref())?;
            let handler = row.work_item_id.as_ref().and_then(|id| handlers.get(id).cloned());
            Some((row, handler))
        })
        .collect()
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
        version: row.base.version,
        request_no: row.request_no.clone(),
        receivable_account_id: row.receivable_account_id.to_string(),
        sales_order_id: row.sales_order_id.to_string(),
        sales_order_no: order_nos.get(row.sales_order_id.as_ref()).cloned().unwrap_or_default(),
        status: row.status,
        created_at: row.base.created_at,
        applicant_user_id: row.created_by.clone(),
        handler_user_id: handler,
        work_item_id: row.work_item_id.clone(),
        amount: row.data.amount,
        data: row.data.clone(),
        invoiced_amount: row.invoiced_amount,
        permission_limited: false,
        sales_owner_user_id: fact.map(|order| order.owner_user_id.clone()),
        business_org_unit_id: fact.map(|order| order.business_org_unit_id.clone()),
    }
}

impl FundsAccess {
    /// 先按正常来源范围取证，仅为可信待处理来源返回可重试状态。
    async fn ensure_request_ready(
        &self,
        row: &SalesInvoiceRequest,
        actor: &AuditActor,
        authorization: &FundsAuthorization,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let allowed =
            self.authorized_sales_targets(authorization, &[row.sales_order_id.to_string()], executor).await?;
        if allowed.is_empty()
            && let Some(readiness) = &self.grant_readiness
            && readiness.pending(&row.base.id, actor.id(), executor).await?
        {
            return Err(Error::Coded(ErrorCode::ResourceAuthorizationNotReady));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use entity_core::BaseModel;
    use erp_core::common::time::Instant;
    use erp_core::ids::{CustomerAccountId, PartyId, ReceivableAccountId};
    use erp_core::money::Amount;
    use erp_finance::entity::receivable::{InvoiceRequestData, InvoiceRequestStatus};

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
        let decided = hydrate_requests(rows, &facts, &handlers);
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

    /// 范围列表及详情必须保留原申请开票资料和实际已开票额，不依赖提交页面缓存。
    #[test]
    fn scoped_request_keeps_saved_invoice_data_and_actual_invoiced_amount() {
        let mut request = request("invoice", "so-a", None);
        request.data.invoice_title = "提交时开票抬头".into();
        request.data.tax_number = "91440101MA00000000".into();
        request.data.invoice_content = "福利用品".into();
        request.data.reason = "按合同开票".into();
        request.invoiced_amount = "3.25".parse().unwrap();
        let row = request_row(&request, None, &HashMap::new(), &HashMap::new());
        assert_eq!(row.data, request.data);
        assert_eq!(row.amount, request.data.amount);
        assert_eq!(row.invoiced_amount, request.invoiced_amount);
        let json = serde_json::to_value(row).unwrap();
        assert_eq!(json["data"]["invoice_title"], "提交时开票抬头");
        assert_eq!(json["data"]["tax_number"], "91440101MA00000000");
        assert_eq!(json["data"]["invoice_content"], "福利用品");
        assert_eq!(json["data"]["reason"], "按合同开票");
        assert_eq!(json["invoiced_amount"], "3.25");
    }

    /// 开票使用当前申请版本，来源与任务身份不依赖浏览器提交缓存或任务负责人映射。
    #[test]
    fn scoped_request_keeps_command_version_and_bound_identity() {
        let mut request = request("invoice", "so-a", Some("work"));
        request.invoiced_amount = "3.25".parse().unwrap();
        let row = request_row(&request, None, &HashMap::new(), &HashMap::new());
        assert_eq!(row.version, 4);
        assert_eq!(row.receivable_account_id, "account");
        assert_eq!(row.work_item_id.as_deref(), Some("work"));
        assert_eq!(request.completion_amount(row.version).unwrap(), "6.74".parse().unwrap());
        let json = serde_json::to_value(&row).unwrap();
        assert_eq!(json["version"], 4);
        assert_eq!(json["receivable_account_id"], "account");
        assert_eq!(json["work_item_id"], "work");

        request.base.version += 1;
        assert!(request.completion_amount(row.version).is_err());
        request.work_item_id = None;
        let refreshed = request_row(&request, None, &HashMap::new(), &HashMap::new());
        assert_eq!(refreshed.version, 5);
        assert!(refreshed.work_item_id.is_none());
        assert_eq!(request.completion_amount(refreshed.version).unwrap(), "6.74".parse().unwrap());
        assert!(serde_json::to_value(refreshed).unwrap()["work_item_id"].is_null());
    }

    /// 复核覆盖全部候选业务版本；页外申请或销售来源变化也会使首拍失效。
    #[test]
    fn request_fingerprint_binds_authorization_and_all_candidate_business_versions() {
        let context = FundsReadContext {
            resource: "sales_invoice_request".into(),
            scope_version: "authorization".into(),
            as_of: Instant::from_unix_secs(1),
        };
        let mut rows = [request("a", "so-a", None), request("b", "so-b", None)];
        let mut facts = HashMap::from([
            ("so-a".into(), fact("owner-a", "org-a", 1)),
            ("so-b".into(), fact("owner-b", "org-b", 2)),
        ]);
        let version = request_version(&context, rows.iter(), &facts);
        rows[1].base.version += 1;
        assert_ne!(version, request_version(&context, rows.iter(), &facts));
        rows[1].base.version -= 1;
        facts.get_mut("so-b").unwrap().version += 1;
        assert_ne!(version, request_version(&context, rows.iter(), &facts));
        facts.get_mut("so-b").unwrap().version -= 1;
        let mut changed = context.clone();
        changed.scope_version = "revoked".into();
        assert_ne!(version, request_version(&changed, rows.iter(), &facts));
        assert_ne!(version, request_version(&context, rows[..1].iter(), &facts));
        assert_eq!(version, request_version(&context, rows.iter(), &facts));
    }
}
