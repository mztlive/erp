use application_core::AuditActor;
use erp_workflow::service::approval::execution::ApprovalRuntimeService;
use erp_workflow::{BpmExt, DocumentType, WorkflowAuthorizationPort};
use persistence_core::{Executor, Transactional};

use super::repository::TimelineRepository;
use super::repository::sources::numbers;
use super::sources::Sources;
use super::{
    SalesTimelineReadService, TaskTimeline, TimelineDocument, TimelinePage, TimelineQuery,
    TimelineSalesSource,
};
use crate::sales_center::access::SalesAccess;
use crate::workbench::WorkbenchReadService;
use crate::{Error, Result};

impl SalesTimelineReadService {
    /// 在实例阅读资格内查询已发起审批的相关动态。
    ///
    /// # 参数
    /// `runtime` 为应用的审批授权服务；其余参数为当前账号、实例与分页。
    /// # 返回
    /// 返回审批对象动态，不把最新动态混入提交材料。
    /// # 错误
    /// 读取前后资格或审批主体变化时拒绝，底层故障继续传播。
    pub async fn approval<A: WorkflowAuthorizationPort>(
        &self,
        runtime: &ApprovalRuntimeService<A>,
        actor: &AuditActor,
        id: &str,
        query: TimelineQuery,
    ) -> Result<TaskTimeline> {
        query.limit()?;
        query.cursor()?;
        let before = runtime.instance_detail(actor, id).await?;
        let kind = DocumentType::try_from_code(
            before.document_type.as_deref().ok_or_else(|| Error::NotFound("审批来源不可用".into()))?,
        )?;
        let document_id =
            before.document_id.clone().ok_or_else(|| Error::NotFound("审批来源不可用".into()))?;
        let submitted_at = before.started_at.and_then(|at| u64::try_from(at).ok());
        let this = self.clone();
        let reader = actor.clone();
        let result = self
            .db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    this.task_document(&reader, kind, &document_id, submitted_at, &query, executor).await
                })
            })
            .await?;
        let after = runtime.instance_detail(actor, id).await?;
        if before.document_id != after.document_id || before.subject_version != after.subject_version {
            return Err(Error::ConflictError("审批资料已变化，请刷新".into()));
        }
        Ok(result)
    }

    /// 以当前任务责任读取办理动态，并独立校验可打开的销售单。
    ///
    /// # 参数
    /// `workbench` 使用应用当前授权器；`actor`、`id`、`query` 为账号、任务和分页。
    /// # 返回
    /// 当前任务单据历史及另行授权的销售单入口。
    /// # 错误
    /// 任务失效、责任越界、参数无效或数据库故障时拒绝。
    pub async fn task<A: WorkflowAuthorizationPort>(
        &self,
        workbench: WorkbenchReadService<A>,
        actor: &AuditActor,
        id: &str,
        query: TimelineQuery,
    ) -> Result<TaskTimeline> {
        query.limit()?;
        query.cursor()?;
        let this = self.clone();
        let actor = actor.clone();
        let id = id.to_owned();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let task = workbench.detail_page(&id, &actor, executor).await?;
                    let kind = match DocumentType::try_from_code(&task.business_object_type) {
                        Ok(kind) => kind,
                        Err(_) => {
                            return Ok(TaskTimeline {
                                timeline: super::dto::page(Vec::new(), &query)?,
                                sales_orders: Vec::new(),
                                submitted_at: None,
                            });
                        },
                    };
                    let submitted_at = if let Some(context) = task.approval_context {
                        this.db
                            .approval_process_instances()
                            .find_by_id(&context.instance_id, executor)
                            .await?
                            .and_then(|instance| u64::try_from(instance.started_at.unix_secs()).ok())
                    } else {
                        None
                    };
                    this.task_document(&actor, kind, &task.business_object_id, submitted_at, &query, executor)
                        .await
                })
            })
            .await
    }

    async fn task_document(
        &self,
        actor: &AuditActor,
        kind: DocumentType,
        id: &str,
        submitted_at: Option<u64>,
        query: &TimelineQuery,
        executor: &mut dyn Executor,
    ) -> Result<TaskTimeline> {
        let mut sources = Sources::new();
        let mut document = TimelineDocument::new(kind, id.into());
        let roots = TimelineRepository { db: &self.db }.roots(&mut document, executor).await?;
        let current_readable = self.access.readable(actor, &document, executor).await?;
        sources.documents.insert(id.into(), document);
        let mut sales_orders = Vec::new();
        for root in roots {
            match SalesAccess::new(self.db.clone(), self.rbac.clone())
                .require_object(actor, "detail", &root, &[], executor)
                .await
            {
                Ok(order) => {
                    sources.add(DocumentType::SalesOrder, root.clone(), Some(&root), None);
                    sales_orders.push(TimelineSalesSource { id: order.base.id, number: order.order_no });
                },
                Err(Error::Forbidden(_) | Error::NotFound(_)) => {},
                Err(error) => return Err(error),
            }
        }
        sources.check_size()?;
        numbers(&mut sources, &self.db, executor).await?;
        // 当前任务资格只提供办理摘要；原单详情链接另行授权。
        let mut timeline = self.read(sources, query, submitted_at, executor).await?;
        restrict_task_details(&mut timeline, id, current_readable);
        Ok(TaskTimeline { timeline, sales_orders, submitted_at })
    }
}

pub(super) fn restrict_task_details(timeline: &mut TimelinePage, id: &str, readable: bool) {
    for event in &mut timeline.items {
        if event.document_id == id && !readable {
            event.href = None;
            event.details.clear();
        }
    }
}
