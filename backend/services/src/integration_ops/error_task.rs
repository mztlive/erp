//! 集成错误任务的登记、列表与只读详情。
//!
//! 人工业务动作只通过 `task_decision` 的 W29 强命令；责任退回、转交和关闭只通过
//! W02 责任 API。本模块不保留旧动作入口。

use database::{AccessControlExt, IntegrationOpsExt, NoTransaction, WorkItemExt};
use entities::integration_ops::{IntegrationErrorTask, IntegrationErrorTaskData, IntegrationErrorTaskId};
use id_generator::next_id;
use validator::Validate;

use super::dto::SortDir;
use super::evidence::{
    error_evidence_policy, evidence_satisfies_policy, EvidenceSubject, IntegrationEvidenceAuthority,
};
use super::producer::{error_owner_role, error_work_item};
use super::{
    ActionBlockerView, CreateErrorTaskRequest, ErrorTaskDetailView, ErrorTaskFilter, ErrorTaskListParams,
    ErrorTaskView, IntegrationOpsService, PageView,
};
use crate::audit::AuditActor;
use crate::errors::{Error, Result};

impl IntegrationOpsService {
    /// 登记集成错误任务。
    ///
    /// # 错误
    /// 请求非法、关联消息不存在或唯一性冲突时返回错误。
    pub async fn create_error_task(
        &self,
        req: CreateErrorTaskRequest,
        actor: &AuditActor,
    ) -> Result<ErrorTaskView> {
        req.validate()?;
        if let Some(message_id) = &req.message_id {
            self.ensure_message_exists(message_id.as_ref()).await?;
        }
        let task = IntegrationErrorTask::new(
            IntegrationErrorTaskId::new(next_id()),
            IntegrationErrorTaskData {
                message_id: req.message_id,
                business_object_id: req.business_object_id,
                error_class: req.error_class,
                owner_role: Some(error_owner_role(req.error_class).to_string()),
                owner_user_id: Some(req.owner_user_id.clone()),
            },
        )?;
        let work_item = error_work_item(&task, &req.owner_user_id)?;
        self.store_error_task(task.clone(), work_item, actor).await?;
        Ok(task.into())
    }

    /// 分页查询集成错误任务列表。
    ///
    /// # 错误
    /// 查询参数非法或仓储查询失败时返回错误。
    pub async fn error_task_list(&self, params: &ErrorTaskListParams) -> Result<PageView<ErrorTaskView>> {
        params.validate()?;
        let query = params.normalized()?;
        let filter = ErrorTaskFilter {
            message_id: query.message_id,
            business_object_id: query.business_object_id,
            error_class: query.error_class,
            status: query.status,
            owner_role: query.owner_role,
            owner_user_id: query.owner_user_id,
            page: query.paging.page,
            page_size: query.paging.page_size,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: matches!(query.paging.sort_dir, SortDir::Asc),
        };
        let page = self
            .db
            .integration_error_tasks()
            .search_error_tasks(&filter, &mut NoTransaction)
            .await?;
        let items = page
            .items
            .into_iter()
            .map(|row| ErrorTaskView {
                id: row.id,
                message_id: row.message_id.map(|id| id.to_string()),
                business_object_id: row.business_object_id,
                error_class: row.error_class,
                status: row.status,
                owner_role: row.owner_role,
                owner_user_id: row.owner_user_id,
                attempt_count: row.attempt_count,
                last_attempt_at: row.last_attempt_at.map(|at| at.unix_secs()),
                last_attempt_summary: row.last_attempt_summary,
                resolution_type: row.resolution_type,
                resolved_at: row.resolved_at.map(|at| at.unix_secs()),
                version: row.version,
                created_at: row.created_at,
            })
            .collect();
        Ok(PageView {
            items,
            total: page.total,
            page: filter.page,
            page_size: filter.page_size,
        })
    }

    /// 查询集成错误任务详情。
    ///
    /// # 错误
    /// 任务不存在时返回错误。
    pub async fn error_task_detail(&self, id: &str) -> Result<ErrorTaskDetailView> {
        let task = self
            .db
            .integration_error_tasks()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("任务不存在".to_string()))?;
        let resolution = task.resolution.clone();
        let work_item = self.find_task_work_item(&task.base.id).await?;
        let subject = EvidenceSubject::error(&task);
        let linked_evidence = self.db.discover_evidence(&subject, &mut NoTransaction).await?;
        let policy = error_evidence_policy(&task);
        let (allowed_actions, action_blockers) =
            error_action_projection(&task, work_item.is_some(), &linked_evidence, &policy);
        let resolution_evidence_policy = (!task.is_terminal() && work_item.is_some()).then_some(policy);
        Ok(ErrorTaskDetailView {
            task: task.into(),
            resolution,
            allowed_actions,
            action_blockers,
            linked_evidence,
            resolution_evidence_policy,
        })
    }

    async fn find_task_work_item(&self, task_id: &str) -> Result<Option<entities::work_item::WorkItem>> {
        let mut items = self
            .db
            .work_items()
            .list_for_integration_error_task(task_id, &mut NoTransaction)
            .await?;
        if items.len() > 1 {
            return Err(Error::ConflictError("错误任务存在多个正式责任关联".to_string()));
        }
        Ok(items.pop())
    }

    async fn ensure_message_exists(&self, id: &str) -> Result<()> {
        self.db
            .inbox_messages()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("关联消息不存在".to_string()))?;
        Ok(())
    }

    async fn store_error_task(
        &self,
        task: IntegrationErrorTask,
        work_item: entities::work_item::WorkItem,
        actor: &AuditActor,
    ) -> Result<()> {
        let audit = actor.clone().resource_log(
            "integration_error_task.create",
            "integration_error_task",
            task.base.id.clone(),
        )?;
        self.run_audited(move |db, session| {
            Box::pin(async move {
                db.integration_error_tasks().create(&task, session).await?;
                db.work_items().create(&work_item, session).await?;
                db.audit_logs().create(&audit, session).await?;
                Ok(())
            })
        })
        .await
    }
}

fn error_action_projection(
    task: &IntegrationErrorTask,
    has_work_item: bool,
    linked_evidence: &[super::ControlledEvidenceRef],
    policy: &super::ResolutionEvidencePolicyView,
) -> (Vec<String>, Vec<ActionBlockerView>) {
    if task.is_terminal() {
        return (Vec::new(), Vec::new());
    }
    if !has_work_item {
        return (
            Vec::new(),
            vec![ActionBlockerView {
                action: "PROCESS".to_string(),
                code: "FORMAL_WORK_ITEM_MISSING".to_string(),
                message: "尚未建立 W29 处理责任，当前错误只能查看。".to_string(),
            }],
        );
    }
    let mut allowed_actions = vec!["QUERY_ORIGINAL_RESULT".to_string(), "ADD_EVIDENCE".to_string()];
    if task.can_replay_original() {
        allowed_actions.push("REPLAY_ORIGINAL".to_string());
    }
    if linked_evidence
        .iter()
        .any(|evidence| evidence.kind == super::ControlledEvidenceKind::BusinessObjectVerification)
    {
        allowed_actions.push("REATTRIBUTE".to_string());
    }
    if linked_evidence
        .iter()
        .any(|evidence| evidence.kind == super::ControlledEvidenceKind::CompensationResult)
    {
        allowed_actions.push("LINK_COMPENSATION".to_string());
    }
    if evidence_satisfies_policy(linked_evidence, policy) {
        allowed_actions.push("RESOLVE".to_string());
        return (allowed_actions, Vec::new());
    }
    (
        allowed_actions,
        vec![ActionBlockerView {
            action: "RESOLVE".to_string(),
            code: "VERIFIED_RESULT_REQUIRED".to_string(),
            message: "取得可验证结果后才能完成任务。".to_string(),
        }],
    )
}
