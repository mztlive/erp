//! 询价的销售安全视图、当前责任采购视图及受控模板资料。
use std::collections::BTreeSet;

use application_core::{AuditActor, CommandFingerprint};
use erp_catalog::ports::supply::CatalogSupplyQueryPort;
use erp_core::common::time::{BusinessDate, Instant};
use erp_core::ids::BusinessDocumentId;
use erp_identity::access_control::{ResolvedScope, ScopeClause};
use erp_identity::service::access_control::resolve::DataScopeService;
use erp_identity::{Permission, SharedRbacService};
use erp_sales::dto::sales_inquiry::{InquiryProcurementView, InquirySalesView};
use erp_sales::entity::sales_inquiry::{InquiryStatus, SalesInquiry};
use erp_sales::repository::sales_inquiry::{InquiryListFilter, InquiryReadScope, InquiryScopeClause};
use erp_sales::service::sales_inquiry::SalesInquiryService;
use erp_supply::inquiry::{InquiryFeeBasis, InquiryQuote, InquirySupplyService};
use erp_support::repository::{DocumentAttachmentRepositoryExt, FileAssetExt};
use erp_workflow::entity::work_item::{WorkItem, WorkItemStatus, WorkItemType, inquiry_subject_version};
use erp_workflow::repository::{WorkItemExt, WorkItemRepositoryExt};
use mongodb::Database;
use persistence_core::NoTransaction;
use serde::{Deserialize, Serialize};

use super::access::SalesInquiryAccess;
use crate::adapters::catalog_supply_query::MongoCatalogSupplyQuery;
use crate::{Error, Result};

/// 分页只接受业务筛选，不接受调用方注入授权范围。
#[derive(Debug, Default, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InquiryListParams {
    pub q: Option<String>,
    pub customer_id: Option<String>,
    pub status: Option<InquiryStatus>,
    pub page: Option<u64>,
    pub scope_version: Option<String>,
    pub task_status: Option<InquiryTaskFilter>,
}

/// 采购列表的任务范围；缺省同时显示待处理与本人已完成记录。
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InquiryTaskFilter {
    Open,
    Completed,
}

impl InquiryTaskFilter {
    fn statuses(filter: Option<Self>) -> &'static [WorkItemStatus] {
        match filter {
            None => &[WorkItemStatus::Open, WorkItemStatus::Completed],
            Some(Self::Open) => &[WorkItemStatus::Open],
            Some(Self::Completed) => &[WorkItemStatus::Completed],
        }
    }
}

/// 采购详情必须引用当前责任任务或本人已完成任务。
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InquiryTaskParams {
    pub task_id: String,
}

/// 费用内部依据读取必须绑定任务与明确反馈轮次。
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InquiryFeeBasisParams {
    pub task_id: String,
    pub feedback_round: u32,
}

/// 对前端公开的任务只包含执行询价所需身份、版本和稳定责任行。
#[derive(Debug, Clone, Serialize)]
pub struct InquiryTaskView {
    pub id: String,
    pub version: u64,
    pub owner_user_id: Option<String>,
    pub line_ids: Vec<String>,
    pub subject_version: String,
    pub status: WorkItemStatus,
}

impl From<&WorkItem> for InquiryTaskView {
    fn from(task: &WorkItem) -> Self {
        Self {
            id: task.base.id.clone(),
            version: task.base.version,
            owner_user_id: task.owner_user_id.clone(),
            line_ids: task.responsibility_scope_ids().to_vec(),
            subject_version: task.subject_version.clone(),
            status: task.status,
        }
    }
}

/// 销售列表独立于内部报价与采购工作草稿。
#[derive(Debug, Serialize)]
pub struct InquiryListView<T> {
    pub items: Vec<T>,
    pub total: i64,
    pub page: u64,
    pub page_size: u32,
    pub scope_version: String,
}

/// 采购详情只开放当前责任行及必要客户和配送信息。
#[derive(Debug, Serialize)]
pub struct ProcurementInquiryDetail {
    #[serde(flatten)]
    pub inquiry: InquiryProcurementView,
    pub work_items: Vec<InquiryTaskView>,
}

impl ProcurementInquiryDetail {
    fn from_task(inquiry: &SalesInquiry, task: &WorkItem, actor: &str) -> Result<Self> {
        let view = if task.status == WorkItemStatus::Open {
            InquiryProcurementView::from_responsibility(
                inquiry,
                task.responsibility_scope_ids(),
                actor,
                Instant::now(),
            )
        } else {
            let requirement = inquiry
                .requirements
                .iter()
                .find(|row| task.subject_version == inquiry_subject_version(row.version))
                .ok_or_else(scope_changed)?;
            InquiryProcurementView::from_completed_task(
                inquiry,
                task.responsibility_scope_ids(),
                actor,
                requirement.version,
                Instant::try_from_unix_secs(
                    i64::try_from(task.base.created_at).map_err(|_| scope_changed())?,
                )?,
                task.completed_at.ok_or_else(scope_changed)?,
            )
            .ok_or_else(|| Error::NotFound("已处理询价的需求或反馈记录缺失，请联系管理员核对".into()))?
        };
        Ok(Self { inquiry: view, work_items: vec![InquiryTaskView::from(task)] })
    }
}

/// 仅传入公司 SKU 稳定身份，不接受商品快照或价格。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InquiryTemplateProductsRequest {
    pub sku_ids: Vec<String>,
}

/// 客户模板商品白名单；不含供给身份、采购价、供应商或内部附件。
#[derive(Debug, Serialize)]
pub struct InquiryTemplateProduct {
    pub sku_id: String,
    pub sku_code: String,
    pub product_name: String,
    pub specification: Option<String>,
    pub unit: String,
}

/// 不缓存对象资格的询价读取用例。
#[derive(Clone)]
pub struct InquiryReadService {
    db: Database,
    rbac: SharedRbacService,
    access: SalesInquiryAccess,
}

impl InquiryReadService {
    /// 装配当前权限与领域读取服务。
    /// # 参数
    /// `db` 为业务库，`rbac` 为当前权限服务。
    /// # 返回
    /// 无缓存读取器。
    /// # 错误
    /// 不执行 I/O，不返回错误。
    pub fn new(db: Database, rbac: SharedRbacService) -> Self {
        Self { access: SalesInquiryAccess::new(db.clone(), rbac.clone()), db, rbac }
    }

    /// 分页读取当前销售责任范围内的安全询价。
    /// # 参数
    /// `actor` 为当前账号，`params` 为业务筛选和范围版本。
    /// # 返回
    /// 返回销售独立投影及稳定分页范围版本。
    /// # 错误
    /// 账号、权限、范围版本或分页非法时拒绝。
    pub async fn sales_list(
        &self,
        actor: &AuditActor,
        params: &InquiryListParams,
    ) -> Result<InquiryListView<InquirySalesView>> {
        let access = self.access.resolve_sales(actor, "list", &mut NoTransaction).await?;
        validate_page(params, &access.scope_version)?;
        let mut filter = list_filter(params);
        filter.access_scope = read_scope(&access.scope, actor.id());
        filter.inquiry_ids = None;
        let page =
            SalesInquiryService::new(self.db.clone()).inquiry_list(&filter, &mut NoTransaction).await?;
        let items = page.items.iter().map(InquirySalesView::from_inquiry).collect();
        if self.access.resolve_sales(actor, "list", &mut NoTransaction).await?.scope_version
            != access.scope_version
        {
            return Err(scope_changed());
        }
        Ok(list_view(items, page.total, params, access.scope_version))
    }

    /// 读取销售安全详情，工作报价不会进入响应。
    /// # 参数
    /// `actor` 为当前账号，`id` 为询价稳定身份。
    /// # 返回
    /// 返回客户需求、已发布结果与可追溯来源。
    /// # 错误
    /// 无权限和不存在统一拒绝，授权源失效时失败关闭。
    pub async fn sales_detail(&self, actor: &AuditActor, id: &str) -> Result<InquirySalesView> {
        let inquiry = self.access.require_sales(actor, "detail", id, &mut NoTransaction).await?;
        Ok(InquirySalesView::from_inquiry(&inquiry))
    }

    /// 分页读取当前及本人已完成询价任务，不授予普通销售数据读取。
    /// # 参数
    /// `actor` 为当前采购，`params` 为业务筛选和范围版本。
    /// # 返回
    /// 每张询价优先展示当前任务，否则展示最近完成任务的历史只读结果。
    /// # 错误
    /// 权限、任务或范围版本变化时拒绝。
    pub async fn procurement_list(
        &self,
        actor: &AuditActor,
        params: &InquiryListParams,
    ) -> Result<InquiryListView<ProcurementInquiryDetail>> {
        self.access.ensure_procurement_permission(actor, "list", &mut NoTransaction).await?;
        let tasks = self.owned_tasks(actor, params.task_status).await?;
        let version = self.procurement_version(actor, &tasks).await?;
        validate_page(params, &version)?;
        let mut filter = list_filter(params);
        filter.access_scope = InquiryReadScope::company();
        filter.inquiry_ids = Some(tasks.iter().map(|task| task.business_object_id.clone()).collect());
        let page =
            SalesInquiryService::new(self.db.clone()).inquiry_list(&filter, &mut NoTransaction).await?;
        let mut items = Vec::with_capacity(page.items.len());
        for inquiry in page.items {
            let task = tasks
                .iter()
                .filter(|task| task.business_object_id == inquiry.base.id)
                .max_by_key(|task| (task.status == WorkItemStatus::Open, task.base.created_at, &task.base.id))
                .ok_or_else(scope_changed)?;
            let (current, task) = self
                .access
                .require_read_task(actor, "list", &inquiry.base.id, &task.base.id, &mut NoTransaction)
                .await?;
            items.push(ProcurementInquiryDetail::from_task(&current, &task, actor.id())?);
        }
        if self.procurement_version(actor, &self.owned_tasks(actor, params.task_status).await?).await?
            != version
        {
            return Err(scope_changed());
        }
        Ok(list_view(items, page.total, params, version))
    }

    /// 按精确责任任务读取当前资料或本人完成的历史结果。
    /// # 参数
    /// `actor` 为当前采购，`id` 为询价，`task_id` 为当前责任任务。
    /// # 返回
    /// 返回责任行白名单与当前任务版本，不返回销售参考价。
    /// # 错误
    /// 关闭、改派、历史证据缺失、账号或权限失效时拒绝。
    pub async fn procurement_detail(
        &self,
        actor: &AuditActor,
        id: &str,
        task_id: &str,
    ) -> Result<ProcurementInquiryDetail> {
        let (inquiry, task) =
            self.access.require_read_task(actor, "detail", id, task_id, &mut NoTransaction).await?;
        ProcurementInquiryDetail::from_task(&inquiry, &task, actor.id())
    }

    /// 内部报价只按当前责任行和独立采购权限返回。
    /// # 参数
    /// `actor` 为当前采购，`id` 与 `task_id` 为精确询价任务。
    /// # 返回
    /// 返回本版本责任行的不可变内部报价历史。
    /// # 错误
    /// 当前任务或内部报价权限失效时拒绝。
    pub async fn internal_quotes(
        &self,
        actor: &AuditActor,
        id: &str,
        task_id: &str,
    ) -> Result<Vec<InquiryQuote>> {
        let (inquiry, task) =
            self.access.require_task(actor, "internal_quote", id, task_id, None, &mut NoTransaction).await?;
        let quotes = InquirySupplyService::new(self.db.clone())
            .quotes(id, inquiry.requirement_version, &mut NoTransaction)
            .await?;
        self.access
            .require_task(actor, "internal_quote", id, task_id, Some(task.base.version), &mut NoTransaction)
            .await?;
        Ok(quotes
            .into_iter()
            .filter(|quote| task.responsibility_scope_ids().contains(&quote.data.scope.line_id))
            .collect())
    }

    /// 按当前责任行及明确反馈轮次读取内部费用依据。
    /// # 参数
    /// `actor` 为当前采购，`id` 为询价，`params` 为任务和反馈轮次。
    /// # 返回
    /// 仅返回任务可见费用对应的内部收款及付款依据。
    /// # 错误
    /// 当前任务或独立内部资料权限失效、轮次非法时拒绝。
    pub async fn fee_bases(
        &self,
        actor: &AuditActor,
        id: &str,
        params: &InquiryFeeBasisParams,
    ) -> Result<Vec<InquiryFeeBasis>> {
        let (inquiry, task) = self
            .access
            .require_task(actor, "internal_quote", id, &params.task_id, None, &mut NoTransaction)
            .await?;
        if params.feedback_round == 0 || params.feedback_round > inquiry.feedback_round.saturating_add(1) {
            return Err(Error::ValidationError("费用反馈轮次无效".into()));
        }
        let view = InquiryProcurementView::from_responsibility(
            &inquiry,
            task.responsibility_scope_ids(),
            actor.id(),
            Instant::now(),
        );
        let fee_ids = view
            .feedbacks
            .iter()
            .filter(|round| round.round == params.feedback_round)
            .flat_map(|round| &round.data.fees)
            .chain(view.feedback_work.iter().flat_map(|work| &work.data.fees))
            .map(|fee| fee.fee_id.clone())
            .collect::<BTreeSet<_>>();
        let bases = InquirySupplyService::new(self.db.clone())
            .fee_bases(id, inquiry.requirement_version, params.feedback_round, &mut NoTransaction)
            .await?;
        self.access
            .require_task(
                actor,
                "internal_quote",
                id,
                &params.task_id,
                Some(task.base.version),
                &mut NoTransaction,
            )
            .await?;
        Ok(bases.into_iter().filter(|basis| fee_ids.contains(&basis.data.fee_id)).collect())
    }

    /// 当前费用任务只能读取本责任费用依据上实际登记的附件。
    /// # 参数
    /// `actor`、`id` 和 `params` 限定当前任务与反馈；其余为依据及文件稳定身份。
    /// # 返回
    /// 本任务可见费用依据和精确附件关系均存在时通过。
    /// # 错误
    /// 任务失效、依据越范围或附件关系不匹配时统一拒绝。
    pub async fn require_fee_basis_attachment(
        &self,
        actor: &AuditActor,
        id: &str,
        params: &InquiryFeeBasisParams,
        basis_id: &str,
        asset_id: &str,
    ) -> Result<()> {
        let bases = self.fee_bases(actor, id, params).await?;
        let basis = bases
            .iter()
            .find(|basis| basis.base.id == basis_id)
            .ok_or_else(|| Error::NotFound("询价费用附件不存在或无权读取".into()))?;
        let links = self
            .db
            .document_attachments()
            .list_by_document(&BusinessDocumentId::new(basis_id), &mut NoTransaction)
            .await?;
        if !basis.data.attachment_ids.iter().any(|id| id == asset_id)
            || !links.iter().any(|link| link.file_asset_id.as_ref() == asset_id)
        {
            return Err(Error::NotFound("询价费用附件不存在或无权读取".into()));
        }
        Ok(())
    }

    /// 按当前商品选择资格重验模板商品，结果只包含对客字段。
    /// # 参数
    /// `actor` 为当前销售，`ids` 为至多 1000 个唯一 SKU 稳定身份。
    /// # 返回
    /// 全部可选商品的客户安全快照，不存在和无权采用统一语义。
    /// # 错误
    /// 任一商品不可选、权限失效或数量超限时整体拒绝。
    pub async fn template_products(
        &self,
        actor: &AuditActor,
        ids: &[String],
    ) -> Result<Vec<InquiryTemplateProduct>> {
        validate_template_ids(ids)?;
        let permission = Permission::parse("sellable_sku:list")?;
        DataScopeService::new(self.db.clone(), self.rbac.clone())
            .resolve_permissions(actor, "sales_inquiry", "create", &[permission], &mut NoTransaction)
            .await?;
        let rows = MongoCatalogSupplyQuery::new(self.db.clone())
            .find_sellable_skus_by_ids(ids, BusinessDate::today(), &mut NoTransaction)
            .await?;
        InquiryTemplateProduct::from_selection(ids, rows)
    }

    async fn owned_tasks(
        &self,
        actor: &AuditActor,
        filter: Option<InquiryTaskFilter>,
    ) -> Result<Vec<WorkItem>> {
        Ok(self
            .db
            .work_items()
            .list_by_type_owners_statuses(
                WorkItemType::SalesInquiryProcessing,
                "sales_inquiry",
                &[actor.id().into()],
                InquiryTaskFilter::statuses(filter),
                &mut NoTransaction,
            )
            .await?)
    }

    async fn procurement_version(&self, actor: &AuditActor, tasks: &[WorkItem]) -> Result<String> {
        let authorization = DataScopeService::new(self.db.clone(), self.rbac.clone())
            .authorization_version(actor, &mut NoTransaction)
            .await?;
        let mut parts = tasks
            .iter()
            .map(|task| format!("{}:{}:{}", task.base.id, task.base.version, task.subject_version))
            .collect::<Vec<_>>();
        parts.sort();
        parts.insert(0, authorization);
        Ok(CommandFingerprint::from_parts(parts).as_str().to_string())
    }
}

fn validate_template_ids(ids: &[String]) -> Result<()> {
    if ids.is_empty()
        || ids.len() > 1000
        || ids.iter().any(|id| id.trim().is_empty())
        || ids.iter().collect::<BTreeSet<_>>().len() != ids.len()
    {
        return Err(Error::ValidationError("请选择 1 至 1000 个不重复的商品".into()));
    }
    Ok(())
}

fn validate_page(params: &InquiryListParams, version: &str) -> Result<()> {
    let page = params.page.unwrap_or(1);
    if page == 0 || page > 500 || params.q.as_ref().is_some_and(|q| q.chars().count() > 100) {
        return Err(Error::ValidationError("询价页码须为 1 至 500，搜索内容不能超过 100 字".into()));
    }
    if (page > 1 && params.scope_version.is_none())
        || params.scope_version.as_ref().is_some_and(|expected| expected != version)
    {
        return Err(scope_changed());
    }
    Ok(())
}

fn list_filter(params: &InquiryListParams) -> InquiryListFilter {
    InquiryListFilter {
        q: params.q.clone(),
        customer_id: params.customer_id.clone(),
        status: params.status,
        page: params.page.unwrap_or(1),
        page_size: 20,
        ..Default::default()
    }
}

fn list_view<T>(
    items: Vec<T>,
    total: i64,
    params: &InquiryListParams,
    scope_version: String,
) -> InquiryListView<T> {
    InquiryListView { items, total, page: params.page.unwrap_or(1), page_size: 20, scope_version }
}

fn scope_changed() -> Error {
    Error::ConflictError("DATA_SCOPE_CHANGED：询价访问范围已变化，请从第一页刷新".into())
}

fn read_scope(scope: &ResolvedScope, user: &str) -> InquiryReadScope {
    InquiryReadScope {
        clauses: scope.role_clauses.iter().map(|clause| read_clause(clause, user)).collect(),
        limit: scope.user_limit.as_ref().map(|clause| read_clause(clause, user)),
    }
}

fn read_clause(clause: &ScopeClause, user: &str) -> InquiryScopeClause {
    if !clause.company && (!clause.settlement_party_ids.is_empty() || !clause.warehouse_ids.is_empty()) {
        return InquiryScopeClause::default();
    }
    InquiryScopeClause {
        company: clause.company,
        owner_user_id: clause.self_owned.then(|| user.to_string()),
        business_org_unit_ids: clause.org_unit_ids.iter().cloned().collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn procurement_history_and_pending_count_use_distinct_task_scopes() {
        assert_eq!(InquiryTaskFilter::statuses(None), &[WorkItemStatus::Open, WorkItemStatus::Completed]);
        assert_eq!(InquiryTaskFilter::statuses(Some(InquiryTaskFilter::Open)), &[WorkItemStatus::Open]);
        assert_eq!(
            InquiryTaskFilter::statuses(Some(InquiryTaskFilter::Completed)),
            &[WorkItemStatus::Completed]
        );
    }
    #[test]
    fn list_scope_preserves_role_union_and_personal_upper_bound() {
        let scope = ResolvedScope {
            role_clauses: vec![
                ScopeClause { self_owned: true, ..Default::default() },
                ScopeClause { org_unit_ids: ["sales-team".into()].into(), ..Default::default() },
            ],
            user_limit: Some(ScopeClause { self_owned: true, ..Default::default() }),
        };
        let actual = read_scope(&scope, "seller");
        assert_eq!(actual.clauses.len(), 2);
        assert_eq!(actual.clauses[0].owner_user_id.as_deref(), Some("seller"));
        assert_eq!(actual.clauses[1].business_org_unit_ids, ["sales-team"]);
        assert_eq!(actual.limit.unwrap().owner_user_id.as_deref(), Some("seller"));
        assert!(
            read_scope(&ResolvedScope { role_clauses: vec![], user_limit: None }, "seller")
                .clauses
                .is_empty()
        );
    }
    #[test]
    fn paging_and_template_selection_fail_closed() {
        assert!(validate_page(&InquiryListParams::default(), "v").is_ok());
        assert!(validate_page(&InquiryListParams { page: Some(2), ..Default::default() }, "v").is_err());
        assert!(
            validate_page(
                &InquiryListParams { page: Some(2), scope_version: Some("v".into()), ..Default::default() },
                "v"
            )
            .is_ok()
        );
        assert!(validate_template_ids(&[]).is_err());
        assert!(validate_template_ids(&["sku".into(), "sku".into()]).is_err());
        assert!(validate_template_ids(&["sku".into()]).is_ok());
    }
}
