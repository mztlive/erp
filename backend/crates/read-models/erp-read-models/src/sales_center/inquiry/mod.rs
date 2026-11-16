//! 询价的销售安全视图、当前责任采购视图及受控模板资料。
use std::collections::BTreeSet;
use std::sync::Arc;

use application_core::AuditActor;
use erp_core::common::time::{BusinessDate, Instant};
use erp_core::ids::BusinessDocumentId;
use erp_identity::{AccessTarget, AuthorizationService};
use erp_permissions::{internal, sales_inquiry as sales_inquiry_permissions};
use erp_sales::dto::sales_inquiry::{InquiryProcurementView, InquirySalesView};
use erp_sales::entity::sales_inquiry::{InquiryStatus, SalesInquiry};
use erp_sales::repository::sales_inquiry::{InquiryListFilter, InquiryReadScope};
use erp_sales::service::sales_inquiry::SalesInquiryService;
use erp_supply::inquiry::{InquiryFeeBasis, InquiryQuote, InquirySupplyService};
use erp_support::repository::{DocumentAttachmentRepositoryExt, FileAssetExt};
use erp_workflow::entity::work_item::{WorkItem, WorkItemStatus, WorkItemType, inquiry_subject_version};
use erp_workflow::repository::{WorkItemExt, WorkItemRepositoryExt};
use mongodb::Database;
use persistence_core::{Executor, Transactional};
use serde::{Deserialize, Serialize};

use crate::catalog_supply::CatalogSupplyRepository;

mod access;
mod authorization;
mod template;
use self::access::SalesInquiryAccess;
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
    pub no_scope: bool,
    pub as_of: i64,
}

/// 采购详情返回已授权询价的完整需求、反馈和当前任务。
#[derive(Debug, Serialize)]
pub struct ProcurementInquiryDetail {
    #[serde(flatten)]
    pub inquiry: InquiryProcurementView,
    pub work_items: Vec<InquiryTaskView>,
}

impl ProcurementInquiryDetail {
    fn from_task(inquiry: &SalesInquiry, task: &WorkItem) -> Result<Self> {
        ensure_task_snapshot(inquiry, task)?;
        let view = if task.status == WorkItemStatus::Open {
            InquiryProcurementView::from_inquiry_at(inquiry, Instant::now())
        } else {
            let requirement = inquiry
                .requirements
                .iter()
                .find(|row| task.subject_version == inquiry_subject_version(row.version))
                .ok_or_else(scope_changed)?;
            InquiryProcurementView::from_completed_task(
                inquiry,
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

/// 已授权当前页仍须保持任务对应的完整业务需求和责任行快照。
fn ensure_task_snapshot(inquiry: &SalesInquiry, task: &WorkItem) -> Result<()> {
    let requirement = if task.status == WorkItemStatus::Open {
        inquiry.current_procurement_requirement()?
    } else {
        inquiry
            .requirements
            .iter()
            .find(|row| task.subject_version == inquiry_subject_version(row.version))
            .ok_or_else(scope_changed)?
    };
    let lines = requirement.data.lines.iter().map(|line| line.line_id.clone()).collect::<Vec<_>>();
    let current = match task.status {
        WorkItemStatus::Open => {
            task.inquiry_task_current(&inquiry.base.id, inquiry.requirement_version, &lines)
        },
        WorkItemStatus::Completed => {
            task.completed_inquiry_current(&inquiry.base.id, requirement.version, &lines)
        },
        _ => false,
    };
    if current { Ok(()) } else { Err(scope_changed()) }
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
    authorization: Arc<AuthorizationService>,
    access: SalesInquiryAccess,
}

impl InquiryReadService {
    /// 装配当前权限与领域读取服务。
    /// # 参数
    /// `db` 为业务库，`authorization` 为当前权限服务。
    /// # 返回
    /// 无缓存读取器。
    /// # 错误
    /// 无。
    pub fn new(db: Database, authorization: Arc<AuthorizationService>) -> Self {
        Self { access: SalesInquiryAccess::new(db.clone(), authorization.clone()), db, authorization }
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
        let this = self.clone();
        let actor = actor.clone();
        let params = params.clone();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move { this.sales_list_stage(&actor, &params, executor).await })
            })
            .await
    }

    async fn sales_list_stage(
        &self,
        actor: &AuditActor,
        params: &InquiryListParams,
        executor: &mut dyn Executor,
    ) -> Result<InquiryListView<InquirySalesView>> {
        let (access, authorization) =
            self.access.resolve_sales(actor, &sales_inquiry_permissions::QUERY_SALES_LIST, executor).await?;
        let version = access.version().to_string();
        validate_page(params, &version)?;
        let mut filter = list_filter(params);
        let scope = InquiryReadScope::from_compiled(access.clone(), authorization.clone());
        filter.inquiry_ids = None;
        let page = SalesInquiryService::new(self.db.clone()).inquiry_list(&filter, &scope, executor).await?;
        authorization.validate(&access, executor).await?;
        let items = page.items.iter().map(InquirySalesView::from_inquiry).collect();
        Ok(list_view(items, page.total, params, version, false))
    }

    /// 读取销售安全详情，工作报价不会进入响应。
    /// # 参数
    /// `actor` 为当前账号，`id` 为询价稳定身份。
    /// # 返回
    /// 返回客户需求、已发布结果与可追溯来源。
    /// # 错误
    /// 无权限和不存在统一拒绝，授权源失效时失败关闭。
    pub async fn sales_detail(&self, actor: &AuditActor, id: &str) -> Result<InquirySalesView> {
        let this = self.clone();
        let actor = actor.clone();
        let id = id.to_string();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move { this.sales_detail_stage(&actor, &id, executor).await })
            })
            .await
    }

    async fn sales_detail_stage(
        &self,
        actor: &AuditActor,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<InquirySalesView> {
        let inquiry = self
            .access
            .require_sales(actor, &sales_inquiry_permissions::QUERY_SALES_DETAIL, id, executor)
            .await?;
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
        let this = self.clone();
        let actor = actor.clone();
        let params = params.clone();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move { this.procurement_list_stage(&actor, &params, executor).await })
            })
            .await
    }

    async fn procurement_list_stage(
        &self,
        actor: &AuditActor,
        params: &InquiryListParams,
        executor: &mut dyn Executor,
    ) -> Result<InquiryListView<ProcurementInquiryDetail>> {
        let tasks = self.owned_tasks(actor, params.task_status, executor).await?;
        let (access, authorization) = authorization::task_query(&self.authorization, actor, executor).await?;
        let version = access.version();
        validate_page(params, &version)?;
        let mut filter = list_filter(params);
        let scope = InquiryReadScope::from_compiled(access.clone(), authorization.clone());
        filter.inquiry_ids = Some(tasks.iter().map(|task| task.business_object_id.clone()).collect());
        let page = SalesInquiryService::new(self.db.clone()).inquiry_list(&filter, &scope, executor).await?;
        let mut items = Vec::with_capacity(page.items.len());
        for inquiry in page.items {
            let task = tasks
                .iter()
                .filter(|task| task.business_object_id == inquiry.base.id)
                .max_by_key(|task| (task.status == WorkItemStatus::Open, task.base.created_at, &task.base.id))
                .ok_or_else(scope_changed)?;
            items.push(ProcurementInquiryDetail::from_task(&inquiry, task)?);
        }
        authorization.validate(&access, executor).await?;
        Ok(list_view(items, page.total, params, version, false))
    }

    /// 按精确责任任务读取完整当前资料或本人完成时的历史结果。
    /// # 参数
    /// `actor` 为当前采购，`id` 为询价，`task_id` 为当前责任任务。
    /// # 返回
    /// 返回完整采购视图与当前任务版本，不返回销售参考价。
    /// # 错误
    /// 关闭、改派、历史证据缺失、账号或权限失效时拒绝。
    pub async fn procurement_detail(
        &self,
        actor: &AuditActor,
        id: &str,
        task_id: &str,
    ) -> Result<ProcurementInquiryDetail> {
        let this = self.clone();
        let actor = actor.clone();
        let id = id.to_string();
        let task_id = task_id.to_string();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move { this.procurement_detail_stage(&actor, &id, &task_id, executor).await })
            })
            .await
    }

    async fn procurement_detail_stage(
        &self,
        actor: &AuditActor,
        id: &str,
        task_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<ProcurementInquiryDetail> {
        let (inquiry, task) = self
            .access
            .require_read_task(
                actor,
                &sales_inquiry_permissions::QUERY_PROCUREMENT_DETAIL,
                id,
                task_id,
                executor,
            )
            .await?;
        ProcurementInquiryDetail::from_task(&inquiry, &task)
    }

    /// 通过当前任务和独立采购权限读取本询价的内部报价。
    /// # 参数
    /// `actor` 为当前采购，`id` 与 `task_id` 为精确询价任务。
    /// # 返回
    /// 返回本需求版本全部商品行的不可变内部报价历史。
    /// # 错误
    /// 当前任务或内部报价权限失效时拒绝。
    pub async fn internal_quotes(
        &self,
        actor: &AuditActor,
        id: &str,
        task_id: &str,
    ) -> Result<Vec<InquiryQuote>> {
        let this = self.clone();
        let actor = actor.clone();
        let id = id.to_string();
        let task_id = task_id.to_string();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move { this.internal_quotes_stage(&actor, &id, &task_id, executor).await })
            })
            .await
    }

    async fn internal_quotes_stage(
        &self,
        actor: &AuditActor,
        id: &str,
        task_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Vec<InquiryQuote>> {
        let (inquiry, _) = self
            .access
            .require_task(actor, &sales_inquiry_permissions::INTERNAL_FEE_BASES, id, task_id, None, executor)
            .await?;
        let quotes = InquirySupplyService::new(self.db.clone())
            .quotes(id, inquiry.requirement_version, executor)
            .await?;
        Ok(quotes)
    }

    /// 通过当前任务授权并按明确反馈轮次读取内部费用依据。
    /// # 参数
    /// `actor` 为当前采购，`id` 为询价，`params` 为任务和反馈轮次。
    /// # 返回
    /// 返回本询价对应轮次费用的内部收款及付款依据。
    /// # 错误
    /// 当前任务或独立内部资料权限失效、轮次非法时拒绝。
    pub async fn fee_bases(
        &self,
        actor: &AuditActor,
        id: &str,
        params: &InquiryFeeBasisParams,
    ) -> Result<Vec<InquiryFeeBasis>> {
        let this = self.clone();
        let actor = actor.clone();
        let id = id.to_string();
        let params = params.clone();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move { this.fee_bases_stage(&actor, &id, &params, executor).await })
            })
            .await
    }

    async fn fee_bases_stage(
        &self,
        actor: &AuditActor,
        id: &str,
        params: &InquiryFeeBasisParams,
        executor: &mut dyn Executor,
    ) -> Result<Vec<InquiryFeeBasis>> {
        let (inquiry, _) = self
            .access
            .require_task(
                actor,
                &sales_inquiry_permissions::INTERNAL_FEE_BASES,
                id,
                &params.task_id,
                None,
                executor,
            )
            .await?;
        if params.feedback_round == 0 || params.feedback_round > inquiry.feedback_round.saturating_add(1) {
            return Err(Error::ValidationError("费用反馈轮次无效".into()));
        }
        let view = InquiryProcurementView::from_inquiry_at(&inquiry, Instant::now());
        let fee_ids = view
            .feedbacks
            .iter()
            .filter(|round| round.round == params.feedback_round)
            .flat_map(|round| &round.data.fees)
            .chain(view.feedback_work.iter().flat_map(|work| &work.data.fees))
            .map(|fee| fee.fee_id.clone())
            .collect::<BTreeSet<_>>();
        let bases = InquirySupplyService::new(self.db.clone())
            .fee_bases(id, inquiry.requirement_version, params.feedback_round, executor)
            .await?;
        Ok(bases.into_iter().filter(|basis| fee_ids.contains(&basis.data.fee_id)).collect())
    }

    /// 当前费用任务只能读取本责任费用依据上实际登记的附件。
    /// # 参数
    /// `actor` 为当前认证操作人，`id` 为询价主键，`params` 限定当前任务与反馈，
    /// `basis_id` 为费用依据主键，`asset_id` 为该依据登记的文件资产主键。
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
        let this = self.clone();
        let actor = actor.clone();
        let id = id.to_string();
        let params = params.clone();
        let basis_id = basis_id.to_string();
        let asset_id = asset_id.to_string();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    this.require_fee_basis_attachment_stage(
                        &actor, &id, &params, &basis_id, &asset_id, executor,
                    )
                    .await
                })
            })
            .await
    }

    async fn require_fee_basis_attachment_stage(
        &self,
        actor: &AuditActor,
        id: &str,
        params: &InquiryFeeBasisParams,
        basis_id: &str,
        asset_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let bases = self.fee_bases_stage(actor, id, params, executor).await?;
        let basis = bases
            .iter()
            .find(|basis| basis.base.id == basis_id)
            .ok_or_else(|| Error::NotFound("询价费用附件不存在或无权读取".into()))?;
        let links = self
            .db
            .document_attachments()
            .list_by_document(&BusinessDocumentId::new(basis_id), executor)
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
        let db = self.db.clone();
        let authorization = self.authorization.clone();
        let actor = actor.clone();
        let ids = ids.to_vec();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    authorization
                        .require(
                            &actor,
                            &internal::SALES_INQUIRY_TEMPLATE_PRODUCTS,
                            AccessTarget::Operation,
                            executor,
                        )
                        .await?;
                    let rows = CatalogSupplyRepository::new(&db)
                        .find_sellable_skus_by_ids(&ids, BusinessDate::today(), executor)
                        .await?;
                    InquiryTemplateProduct::from_selection(&ids, rows)
                })
            })
            .await
    }

    async fn owned_tasks(
        &self,
        actor: &AuditActor,
        filter: Option<InquiryTaskFilter>,
        executor: &mut dyn Executor,
    ) -> Result<Vec<WorkItem>> {
        Ok(self
            .db
            .work_items()
            .list_by_type_owners_statuses(
                WorkItemType::SalesInquiryProcessing,
                "sales_inquiry",
                &[actor.id().into()],
                InquiryTaskFilter::statuses(filter),
                executor,
            )
            .await?)
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
    no_scope: bool,
) -> InquiryListView<T> {
    InquiryListView {
        items,
        total,
        page: params.page.unwrap_or(1),
        page_size: 20,
        scope_version,
        no_scope,
        as_of: Instant::now().unix_secs(),
    }
}

fn scope_changed() -> Error {
    Error::ConflictError("DATA_SCOPE_CHANGED：询价访问范围已变化，请从第一页刷新".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn inquiry_task_fixture() -> (SalesInquiry, WorkItem) {
        use erp_core::ids::WorkItemId;
        use erp_sales::entity::sales_inquiry::{
            InquiryCreateData, InquiryFulfillmentKind, InquiryRequirementData, InquiryRequirementLine,
            InquiryRequirementSnapshot,
        };
        use erp_workflow::entity::work_item::{AssignmentSource, WorkItemData, WorkItemPriority};
        let data = InquiryRequirementData {
            note: String::new(),
            lines: vec![InquiryRequirementLine {
                line_id: "line".into(),
                sku_id: "sku".into(),
                sku_revision_id: "revision".into(),
                sku_code: "sku-code".into(),
                product_name: "商品".into(),
                specification: String::new(),
                unit_code: "piece".into(),
                quantity_scale: 0,
                quantity: "1".parse().unwrap(),
                fulfillment_kind: InquiryFulfillmentKind::Virtual,
                delivery_due_at: Instant::from_unix_secs(1_900_000_000),
                delivery_terms: String::new(),
                sales_reference_price_gross: None,
                allocations: Vec::new(),
            }],
        };
        let mut inquiry = SalesInquiry::create(
            "inquiry".into(),
            "INQ-1".into(),
            InquiryCreateData {
                customer_id: "customer".into(),
                customer_name: "客户".into(),
                sales_owner_user_id: "seller".into(),
                business_org_unit_id: "org".into(),
                deadline_at: Instant::from_unix_secs(1_900_000_000),
                copied_from_inquiry_id: None,
                draft: data.clone(),
            },
        )
        .unwrap();
        inquiry.requirement_version = 1;
        inquiry.status = InquiryStatus::ProcurementProcessing;
        inquiry.requirements.push(InquiryRequirementSnapshot {
            version: 1,
            submitted_by: "seller".into(),
            submitted_at: Instant::now(),
            change_reason: String::new(),
            data,
        });
        let task = WorkItem::new_with_responsibility_scope(
            WorkItemId::new("task"),
            WorkItemData {
                work_item_type: WorkItemType::SalesInquiryProcessing,
                business_object_type: "sales_inquiry".into(),
                business_object_id: "inquiry".into(),
                subject_version: inquiry_subject_version(1),
                owner_role: "inquiry_buyer".into(),
                owner_organization_id: "org".into(),
                owner_user_id: "buyer".into(),
                assignment_source: AssignmentSource::SystemRule,
                priority: WorkItemPriority::Normal,
                due_at: None,
                reason_code: None,
                impact_summary: None,
            },
            "inquiry:buyer",
            vec!["line".into()],
        )
        .unwrap();
        (inquiry, task)
    }

    /// 分页后只对当前结果检查真实需求与责任行，不再逐项重新发起授权。
    #[test]
    fn procurement_page_keeps_open_and_completed_task_snapshot_guards() {
        let (inquiry, mut task) = inquiry_task_fixture();
        assert!(ensure_task_snapshot(&inquiry, &task).is_ok());
        task.subject_version = inquiry_subject_version(2);
        assert!(ensure_task_snapshot(&inquiry, &task).is_err());
        task.subject_version = inquiry_subject_version(1);
        task.complete_by_domain_command("buyer", Instant::now()).unwrap();
        assert!(ensure_task_snapshot(&inquiry, &task).is_ok());
        task.completed_by = None;
        assert!(ensure_task_snapshot(&inquiry, &task).is_err());
    }

    #[test]
    fn procurement_page_rejects_deleted_or_unrelated_task() {
        let (inquiry, mut task) = inquiry_task_fixture();
        task.business_object_id = "another-inquiry".into();
        assert!(ensure_task_snapshot(&inquiry, &task).is_err());
        task.business_object_id = inquiry.base.id.clone();
        task.base.deleted_at = 1;
        assert!(ensure_task_snapshot(&inquiry, &task).is_err());
    }

    #[test]
    fn list_view_distinguishes_missing_scope_from_empty_business_records() {
        let params = InquiryListParams::default();
        let unavailable = list_view(Vec::<String>::new(), 0, &params, "v1".into(), true);
        let empty = list_view(Vec::<String>::new(), 0, &params, "v1".into(), false);
        assert!(unavailable.no_scope);
        assert!(!empty.no_scope);
        assert_eq!(empty.total, 0);
        assert!(empty.as_of > 0);
    }

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

    #[test]
    fn paging_rejects_replaced_or_changed_authorization_versions() {
        for page in [1, 2] {
            let params = InquiryListParams {
                page: Some(page),
                scope_version: Some("previous-authorization-version".into()),
                ..Default::default()
            };
            assert!(matches!(
                validate_page(&params, "current-authorization-version"),
                Err(Error::ConflictError(message)) if message.contains("DATA_SCOPE_CHANGED")
            ));
        }
    }
}
