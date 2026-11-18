use async_trait::async_trait;
use bpm::SubjectRef;
use bpm::model::types::{ApprovalNodeExecutionStatus, ApprovalProcessInstanceStatus};
use erp_finance::entity::receivable::InvoiceRequestStatus;
use erp_finance::repository::extensions::ReceivableExt;
use erp_finance::repository::receivable::SalesInvoiceRequestRepositoryExt;
use erp_identity::entity::resource_access::event::{GrantEvent, GrantEventPayload};
use erp_identity::entity::resource_access::{GrantTarget, ResourceKind, SourceUpdate};
use erp_identity::ports::ResourceGrantFacts;
use erp_identity::{Error, Result};
use erp_procurement::repository::extensions::PurchaseOrderExt;
use erp_sales::repository::extensions::SalesOrderExt;
use erp_workflow::entity::work_item::WorkItemType;
use erp_workflow::repository::{BpmExt, BpmWorkflowRepository, WorkItemCommandExt, WorkItemExt};
use mongodb::Database;
use persistence_core::Executor;

use super::rules::{AssignmentEvidence, InvoiceEvidence};

pub(crate) struct Facts {
    pub db: Database,
    pub event: Option<GrantEvent>,
}
impl Facts {
    // 用传入的执行器从正式业务记录计算授权来源的完整目标，供发布和就绪查询共用；证据不匹配、事实版本回退或读取失败时返回错误，不生成空目标。
    pub(crate) async fn compute(
        db: &Database,
        event: &GrantEvent,
        ex: &mut dyn Executor,
    ) -> Result<SourceUpdate> {
        let (target, version) = Self::target(db, event, ex).await?;
        if event.source != event.evidence.payload.source(&event.evidence.recipient)?
            || version < event.evidence.fact_version
        {
            return Err(conflict());
        }

        Ok(SourceUpdate {
            key: event.source.clone(),
            rule_version: 1,
            fact_version: version,
            targets: vec![target],
            reason: "正式业务分配自动读取授权".into(),
        })
    }

    async fn target(db: &Database, event: &GrantEvent, ex: &mut dyn Executor) -> Result<(GrantTarget, u64)> {
        Ok(match &event.evidence.payload {
            GrantEventPayload::InvoiceTaskReassigned { work_item_id, command_receipt_id } => {
                Self::reassignment(db, event, work_item_id, command_receipt_id, ex).await?
            },
            GrantEventPayload::InvoiceApproved { invoice_request_id, approval_instance_id, work_item_id } => {
                Self::invoice(db, event, invoice_request_id, approval_instance_id, work_item_id, ex).await?
            },
            GrantEventPayload::SalesOrderApprovalAssigned {
                sales_order_id,
                approval_instance_id,
                node_id,
                assignment_id,
            } => {
                Self::approval(
                    db,
                    event,
                    GrantTarget { resource: ResourceKind::SalesOrder, id: sales_order_id.clone() },
                    approval_instance_id,
                    node_id,
                    assignment_id,
                    ex,
                )
                .await?
            },
            GrantEventPayload::PurchaseOrderApprovalAssigned {
                purchase_order_id,
                approval_instance_id,
                node_id,
                assignment_id,
            } => {
                Self::approval(
                    db,
                    event,
                    GrantTarget { resource: ResourceKind::PurchaseOrder, id: purchase_order_id.clone() },
                    approval_instance_id,
                    node_id,
                    assignment_id,
                    ex,
                )
                .await?
            },
        })
    }

    async fn reassignment(
        db: &Database,
        event: &GrantEvent,
        task_id: &str,
        receipt_id: &str,
        ex: &mut dyn Executor,
    ) -> Result<(GrantTarget, u64)> {
        let task = db.work_items().find_by_id(task_id, ex).await?.ok_or_else(conflict)?;
        let receipt =
            db.work_item_command_receipts().find_by_id(receipt_id, ex).await?.ok_or_else(conflict)?;
        let expected = receipt.invoice_grant(&task).map_err(|_| conflict())?.ok_or_else(conflict)?;
        if expected != event.evidence {
            return Err(conflict());
        }

        let request = db.sales_invoice_requests().find_for_task(task_id, ex).await?.ok_or_else(conflict)?;
        let subject = SubjectRef::new("sales_invoice_request", &request.base.id).map_err(|_| conflict())?;
        let approval = BpmWorkflowRepository::new(db)
            .find_latest_by_subject(&subject, ex)
            .await?
            .ok_or_else(conflict)?;

        let (target, _) = Self::invoice(db, event, &request.base.id, &approval.base.id, task_id, ex).await?;
        Ok((target, task.base.version))
    }

    async fn invoice(
        db: &Database,
        event: &GrantEvent,
        request_id: &str,
        approval_id: &str,
        task_id: &str,
        ex: &mut dyn Executor,
    ) -> Result<(GrantTarget, u64)> {
        let request = db.sales_invoice_requests().find_by_id(request_id, ex).await?.ok_or_else(conflict)?;
        let approval =
            db.approval_process_instances().find_by_id(approval_id, ex).await?.ok_or_else(conflict)?;
        let task = db.work_items().find_by_id(task_id, ex).await?.ok_or_else(conflict)?;
        let account = db
            .receivable_accounts()
            .find_by_id(request.receivable_account_id.as_ref(), ex)
            .await?
            .ok_or_else(conflict)?;

        InvoiceEvidence {
            approved: approval.status == ApprovalProcessInstanceStatus::Approved,
            request_approved: matches!(
                request.status,
                InvoiceRequestStatus::Approved | InvoiceRequestStatus::Completed
            ),
            approval_kind: approval.subject.subject_kind(),
            approval_request: approval.subject.subject_id(),
            request_id,
            approval_version: approval.subject_version,
            request_version: request.approval_subject_version,
            request_task: request.work_item_id.as_deref(),
            task_id,
            invoice_task: task.work_item_type == WorkItemType::SalesInvoiceExecution,
            task_kind: &task.business_object_type,
            task_account: &task.business_object_id,
            request_account: request.receivable_account_id.as_ref(),
            account_id: &account.base.id,
            request_sales: request.sales_order_id.as_ref(),
            account_sales: account.sales_order_id.as_ref(),
            historical_recipient: task.responsibility_actor_ids.contains(&event.evidence.recipient),
        }
        .validate()?;

        Ok((
            GrantTarget { resource: ResourceKind::SalesOrder, id: request.sales_order_id.to_string() },
            request.base.version,
        ))
    }
    async fn approval(
        db: &Database,
        event: &GrantEvent,
        target: GrantTarget,
        instance_id: &str,
        node: &str,
        assignment: &str,
        ex: &mut dyn Executor,
    ) -> Result<(GrantTarget, u64)> {
        let instance =
            db.approval_process_instances().find_by_id(instance_id, ex).await?.ok_or_else(conflict)?;
        let execution =
            db.approval_node_executions().find_by_id(assignment, ex).await?.ok_or_else(conflict)?;

        AssignmentEvidence {
            subject_kind: instance.subject.subject_kind(),
            subject_id: instance.subject.subject_id(),
            resource: target.resource,
            target_id: &target.id,
            instance_id,
            execution_instance: execution.process_instance_id.as_ref(),
            node,
            execution_node: &execution.node_key,
            blocked: execution.status == ApprovalNodeExecutionStatus::Blocked,
            participant: execution.has_participant(&event.evidence.recipient),
        }
        .validate()?;
        Ok((target, instance.base.version))
    }
}
fn conflict() -> Error {
    Error::ConflictError("自动授权业务关联或正式分配事实不一致".into())
}
#[async_trait]
impl ResourceGrantFacts for Facts {
    async fn validate_targets(&self, targets: &[GrantTarget], ex: &mut dyn Executor) -> Result<()> {
        validate_targets(&self.db, targets, ex).await
    }
    async fn validate_source(&self, update: &SourceUpdate, ex: &mut dyn Executor) -> Result<()> {
        let expected = Self::compute(&self.db, self.event.as_ref().ok_or_else(conflict)?, ex).await?;
        if expected.key != update.key
            || expected.rule_version != update.rule_version
            || expected.fact_version != update.fact_version
            || expected.targets != update.targets
        {
            return Err(conflict());
        }
        Ok(())
    }
}

pub(crate) async fn validate_targets(
    db: &Database,
    targets: &[GrantTarget],
    ex: &mut dyn Executor,
) -> Result<()> {
    for target in targets {
        let exists = match target.resource {
            ResourceKind::SalesOrder => db.sales_orders().find_by_id(&target.id, ex).await?.is_some(),
            ResourceKind::PurchaseOrder => db.purchase_orders().find_by_id(&target.id, ex).await?.is_some(),
        };
        if !exists {
            return Err(Error::ConflictError("找不到要授权查看的销售单或采购单，请核对所选单据".into()));
        }
    }
    Ok(())
}
