//! 已授权命令调用拥有领域规则及工作项写入。

use application_core::AuditActor;
use erp_core::common::time::Instant;
use erp_identity::service::business_ownership::required_business_org;
use erp_identity::{AccessTarget, AccountDirectoryService};
use erp_permissions::{internal as internal_permissions, sales_inquiry as sales_inquiry_permissions};
use erp_sales::entity::sales_inquiry::{InquiryReassignmentData, SalesInquiry};
use erp_workflow::entity::work_item::WorkItem;
use erp_workflow::repository::WorkItemExt;
use persistence_core::Executor;

use super::InquiryCommand;
use crate::sales_inquiry::SalesInquiryProcess;
use crate::sales_inquiry::dto::{ReassignInquiryCommand, SaveInquiryCommand};
use crate::sales_inquiry::eligibility::normalize_requirement;
use crate::{Error, Result};

impl SalesInquiryProcess {
    pub(in crate::sales_inquiry) async fn apply_command(
        &self,
        actor: &AuditActor,
        id: &str,
        command: &InquiryCommand,
        executor: &mut dyn Executor,
    ) -> Result<(SalesInquiry, Vec<String>)> {
        if let InquiryCommand::Create(req) = command {
            return Ok((self.create_inquiry(actor, req, executor).await?, Vec::new()));
        }
        if let InquiryCommand::Renew(req) = command {
            return self.renew_work(actor, id, req, executor).await;
        }

        if let Some((task_id, version)) = command.task() {
            let permission = command.procurement_permission()?;
            let (mut inquiry, mut task) =
                self.access().require_task(actor, &permission, id, task_id, Some(version), executor).await?;
            self.apply_procurement(actor, &mut inquiry, &mut task, command, executor).await?;
            let tasks = self.active_tasks(id, executor).await?.into_iter().map(|task| task.base.id).collect();
            return Ok((inquiry, tasks));
        }

        let permission = command.sales_permission()?;
        let mut inquiry = self.access().require_sales(actor, &permission, id, executor).await?;
        let tasks = self.apply_sales(actor, &mut inquiry, command, executor).await?;
        Ok((inquiry, tasks))
    }

    async fn apply_sales(
        &self,
        actor: &AuditActor,
        inquiry: &mut SalesInquiry,
        command: &InquiryCommand,
        executor: &mut dyn Executor,
    ) -> Result<Vec<String>> {
        let now = Instant::now();
        match command {
            InquiryCommand::Save(req) => {
                self.save_requirement(actor, inquiry, req, executor).await?;
            },
            InquiryCommand::Submit(req) => {
                self.access()
                    .require(actor, &sales_inquiry_permissions::COMMANDS_SALES_SUBMIT, inquiry, executor)
                    .await?;
                self.require_customer(actor, &inquiry.customer_id, executor).await?;
                normalize_requirement(&self.db, self.services.catalog.as_ref(), &mut inquiry.draft, executor)
                    .await?;

                inquiry.submit(req.expected_version, actor.id(), req.reason.clone(), now)?;
                return self.replace_tasks(actor, inquiry, executor).await;
            },
            InquiryCommand::Accept(req) => {
                inquiry.accept(req.expected_version, actor.id(), req.feedback_round, now)?
            },
            InquiryCommand::Reinquire(req) => {
                let mut data = req.data.clone();
                if let Some(requirement) = &mut data.requirement {
                    self.access()
                        .require(
                            actor,
                            &internal_permissions::SALES_INQUIRY_REINQUIRE_DRAFT,
                            inquiry,
                            executor,
                        )
                        .await?;
                    normalize_requirement(&self.db, self.services.catalog.as_ref(), requirement, executor)
                        .await?;
                }

                inquiry.reinquire(req.expected_version, actor.id(), data, now)?;
                return self.replace_tasks(actor, inquiry, executor).await;
            },
            InquiryCommand::Close(req) => {
                inquiry.close(req.expected_version, actor.id(), req.reason.clone(), now)?;
                self.close_tasks(actor, &inquiry.base.id, &req.reason, executor).await?;
            },
            InquiryCommand::AllocateFees(req) => self.apply_fee_allocations(inquiry, req, executor).await?,
            _ => return Err(Error::Internal("询价销售命令分派错误".into())),
        }
        Ok(Vec::new())
    }

    async fn save_requirement(
        &self,
        actor: &AuditActor,
        inquiry: &mut SalesInquiry,
        request: &SaveInquiryCommand,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        self.access()
            .require(actor, &sales_inquiry_permissions::COMMANDS_SALES_SAVE, inquiry, executor)
            .await?;
        let mut data = request.data.clone();
        normalize_requirement(&self.db, self.services.catalog.as_ref(), &mut data, executor).await?;
        self.require_customer(actor, &inquiry.customer_id, executor).await?;
        Ok(inquiry.save_draft(request.expected_version, data, request.deadline_at)?)
    }

    async fn apply_procurement(
        &self,
        actor: &AuditActor,
        inquiry: &mut SalesInquiry,
        task: &mut WorkItem,
        command: &InquiryCommand,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        match command {
            InquiryCommand::Supplement(req) => {
                inquiry.request_supplement(
                    req.expected_version,
                    actor.id(),
                    req.reason.clone(),
                    Instant::now(),
                )?;
                return self.close_tasks(actor, &inquiry.base.id, &req.reason, executor).await;
            },
            InquiryCommand::Publish(req) => {
                return self.publish_work(actor, inquiry, req.expected_version, executor).await;
            },
            _ => self.change_task(actor, inquiry, task, command, executor).await?,
        }

        self.db.work_items().update(task, executor).await?;
        Ok(())
    }

    async fn change_task(
        &self,
        actor: &AuditActor,
        inquiry: &mut SalesInquiry,
        task: &mut WorkItem,
        command: &InquiryCommand,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let now = Instant::now();
        match command {
            InquiryCommand::SaveFeedback(req) => inquiry.save_feedback_work(
                req.expected_version,
                actor.id(),
                task.responsibility_scope_ids().to_vec(),
                req.data.clone(),
                now,
            )?,
            InquiryCommand::InternalQuote(req) => {
                self.append_internal_quote(actor, inquiry, task, req.expected_version, &req.quote, executor)
                    .await?
            },
            InquiryCommand::ImportQuotes(req) => {
                self.append_imported_quotes(actor, inquiry, task, req, executor).await?
            },
            InquiryCommand::FeeBasis(req) => {
                self.append_fee_basis(actor, inquiry, task, req, executor).await?
            },
            InquiryCommand::Reassign(req) => {
                return self.reassign_task(actor, inquiry, task, req, executor).await;
            },
            _ => return Err(Error::Internal("询价采购命令分派错误".into())),
        }

        task.record_activity(now)?;
        Ok(())
    }

    /// 改派只按真实目标账号和原任务版本证明接收资格，责任行及组织规则独立保留。
    async fn reassign_task(
        &self,
        actor: &AuditActor,
        inquiry: &mut SalesInquiry,
        task: &mut WorkItem,
        request: &ReassignInquiryCommand,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        if self.active_tasks(&inquiry.base.id, executor).await?.iter().any(|other| {
            other.base.id != task.base.id && other.owner_user_id.as_deref() == Some(&request.owner_user_id)
        }) {
            return Err(Error::BusinessLogicError(
                "目标采购已负责本询价其他任务，请保持一人一组责任行".into(),
            ));
        }

        let account = AccountDirectoryService::new(self.db.clone())
            .account(&request.owner_user_id, executor)
            .await?
            .ok_or_else(|| Error::BusinessLogicError("询价责任采购账号已失效".into()))?;
        let recipient = AuditActor::new(account.id, account.login_account, account.kind);
        self.authorization
            .require(
                &recipient,
                &internal_permissions::WORK_ITEM_RECIPIENT,
                AccessTarget::Task { id: task.base.id.clone(), version: task.base.version.to_string() },
                executor,
            )
            .await?;
        required_business_org(&self.db, &request.owner_user_id, executor).await?;

        let now = Instant::now();
        inquiry.reassign_work(
            request.expected_version,
            actor.id(),
            InquiryReassignmentData {
                previous_owner: actor.id().to_string(),
                target_owner: request.owner_user_id.clone(),
                line_ids: task.responsibility_scope_ids().to_vec(),
                reason: request.reason.clone(),
            },
            now,
        )?;
        Ok(task.reassign(&request.owner_user_id, now)?)
    }

    async fn publish_work(
        &self,
        actor: &AuditActor,
        inquiry: &mut SalesInquiry,
        version: u64,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let tasks = self.active_tasks(&inquiry.base.id, executor).await?;
        let responsibilities = tasks
            .iter()
            .map(|task| {
                (task.owner_user_id.clone().unwrap_or_default(), task.responsibility_scope_ids().to_vec())
            })
            .collect::<Vec<_>>();
        let data = inquiry.merge_feedback_work(&responsibilities)?;
        inquiry.publish(version, actor.id(), data, Instant::now())?;

        self.complete_tasks(inquiry, executor).await
    }
}
