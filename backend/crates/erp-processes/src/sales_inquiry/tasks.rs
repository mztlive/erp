//! 询价责任工作项与聚合状态在同一事务内变更。

use application_core::AuditActor;
use erp_core::common::time::Instant;
use erp_core::ids::WorkItemId;
use erp_sales::entity::sales_inquiry::SalesInquiry;
use erp_workflow::entity::work_item::{
    AssignmentSource, InquiryTaskAssignment, WorkItem, WorkItemCloseData, WorkItemData, WorkItemPriority,
    WorkItemType, inquiry_subject_version,
};
use erp_workflow::repository::{WorkItemExt, WorkItemRepositoryExt};
use id_generator::next_id;
use persistence_core::Executor;

use super::SalesInquiryProcess;
use crate::{Error, Result};

impl SalesInquiryProcess {
    pub(super) async fn ensure_reassignment_target(
        &self,
        inquiry_id: &str,
        task_id: &str,
        owner: &str,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        if self
            .active_tasks(inquiry_id, executor)
            .await?
            .iter()
            .any(|task| task.base.id != task_id && task.owner_user_id.as_deref() == Some(owner))
        {
            return Err(Error::BusinessLogicError(
                "目标采购已负责本询价其他任务，请保持一人一组责任行".into(),
            ));
        }
        self.access().assignment_owner(owner, executor).await?;
        Ok(())
    }
    pub(super) async fn active_tasks(&self, id: &str, executor: &mut dyn Executor) -> Result<Vec<WorkItem>> {
        let mut items = self
            .db
            .work_items()
            .list_active_by_object("sales_inquiry", id, executor)
            .await?
            .into_iter()
            .filter(|item| item.work_item_type == WorkItemType::SalesInquiryProcessing)
            .collect::<Vec<_>>();
        items.sort_by(|a, b| a.base.id.cmp(&b.base.id));
        Ok(items)
    }

    pub(super) async fn replace_tasks(
        &self,
        actor: &AuditActor,
        inquiry: &mut SalesInquiry,
        executor: &mut dyn Executor,
    ) -> Result<Vec<String>> {
        let assignments = self.resolve_assignments(inquiry, executor).await?;
        let owners =
            assignments.iter().map(|(assignment, _)| assignment.owner_user_id.clone()).collect::<Vec<_>>();
        inquiry.assign_shared_fee_owner(None, &owners)?;
        self.close_tasks(actor, &inquiry.base.id, "询价需求重新提交或重询，旧任务冻结", executor).await?;
        let mut ids = Vec::new();
        for (assignment, organization) in assignments {
            let item = new_task(inquiry, assignment, organization)?;
            self.db.work_items().create(&item, executor).await?;
            ids.push(item.base.id);
        }
        Ok(ids)
    }

    pub(super) async fn close_tasks(
        &self,
        actor: &AuditActor,
        id: &str,
        reason: &str,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        for mut item in self.active_tasks(id, executor).await? {
            item.close(actor.id(), WorkItemCloseData { close_reason: reason.to_string() }, Instant::now())?;
            self.db.work_items().update(&mut item, executor).await?;
        }
        Ok(())
    }

    pub(super) async fn complete_tasks(&self, id: &str, executor: &mut dyn Executor) -> Result<()> {
        for mut item in self.active_tasks(id, executor).await? {
            let owner = item.owner_user_id.clone().expect("开放询价责任任务必有当前负责人");
            self.access().assignment_owner(&owner, executor).await?;
            item.complete_by_domain_command(owner, Instant::now())?;
            self.db.work_items().update(&mut item, executor).await?;
        }
        Ok(())
    }
}

fn new_task(
    inquiry: &SalesInquiry,
    assignment: InquiryTaskAssignment,
    organization: String,
) -> Result<WorkItem> {
    let key = format!("inquiry:{}:{}", inquiry.requirement_version, assignment.owner_user_id);
    Ok(WorkItem::new_with_responsibility_scope(
        WorkItemId::new(next_id()),
        WorkItemData {
            work_item_type: WorkItemType::SalesInquiryProcessing,
            business_object_type: "sales_inquiry".into(),
            business_object_id: inquiry.base.id.clone(),
            subject_version: inquiry_subject_version(inquiry.requirement_version),
            owner_role: "inquiry_buyer".into(),
            owner_organization_id: organization,
            owner_user_id: assignment.owner_user_id,
            assignment_source: AssignmentSource::SystemRule,
            priority: WorkItemPriority::Normal,
            due_at: Some(inquiry.deadline_at),
            reason_code: Some("SALES_INQUIRY_SUBMITTED".into()),
            impact_summary: Some("询价需求待处理".into()),
        },
        key,
        assignment.line_ids,
    )?)
}
