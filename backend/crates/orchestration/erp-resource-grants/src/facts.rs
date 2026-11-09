use async_trait::async_trait;
use bpm::model::types::{ApprovalNodeExecutionStatus, ApprovalProcessInstanceStatus};
use erp_finance::entity::receivable::InvoiceRequestStatus;
use erp_finance::repository::extensions::ReceivableExt;
use erp_identity::entity::resource_access::event::{GrantEvent, GrantEventPayload};
use erp_identity::entity::resource_access::{GrantTarget, ResourceKind, SourceUpdate};
use erp_identity::ports::ResourceGrantFacts;
use erp_identity::{Error, Result};
use erp_procurement::repository::extensions::PurchaseOrderExt;
use erp_sales::repository::extensions::SalesOrderExt;
use erp_workflow::entity::work_item::WorkItemType;
use erp_workflow::repository::{BpmExt, WorkItemExt};
use mongodb::Database;
use persistence_core::Executor;

use super::rules::{AssignmentEvidence, InvoiceEvidence};

pub(crate) struct Facts {
    pub db: Database,
    pub event: Option<GrantEvent>,
}
impl Facts {
    /// 读取当前事实并计算来源完整目标。
    /// # 参数
    /// event 为正式事件，ex 为发布执行器。
    /// # 返回
    /// 经关联校验的完整来源结果。
    /// # 错误
    /// 读取失败、缺失或关联错配时拒绝。
    pub(crate) async fn compute(&self, event: &GrantEvent, ex: &mut dyn Executor) -> Result<SourceUpdate> {
        let (target, version) = match &event.evidence.payload {
            GrantEventPayload::InvoiceApproved { invoice_request_id, approval_instance_id, work_item_id } => {
                self.invoice(event, invoice_request_id, approval_instance_id, work_item_id, ex).await?
            },
            GrantEventPayload::SalesOrderApprovalAssigned {
                sales_order_id,
                approval_instance_id,
                node_id,
                assignment_id,
            } => {
                self.approval(
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
                self.approval(
                    event,
                    GrantTarget { resource: ResourceKind::PurchaseOrder, id: purchase_order_id.clone() },
                    approval_instance_id,
                    node_id,
                    assignment_id,
                    ex,
                )
                .await?
            },
        };
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

    async fn invoice(
        &self,
        event: &GrantEvent,
        request_id: &str,
        approval_id: &str,
        task_id: &str,
        ex: &mut dyn Executor,
    ) -> Result<(GrantTarget, u64)> {
        let request =
            self.db.sales_invoice_requests().find_by_id(request_id, ex).await?.ok_or_else(conflict)?;

        let approval =
            self.db.approval_process_instances().find_by_id(approval_id, ex).await?.ok_or_else(conflict)?;

        let task = self.db.work_items().find_by_id(task_id, ex).await?.ok_or_else(conflict)?;

        let account = self
            .db
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
        &self,
        event: &GrantEvent,
        target: GrantTarget,
        instance_id: &str,
        node: &str,
        assignment: &str,
        ex: &mut dyn Executor,
    ) -> Result<(GrantTarget, u64)> {
        let instance =
            self.db.approval_process_instances().find_by_id(instance_id, ex).await?.ok_or_else(conflict)?;
        let execution =
            self.db.approval_node_executions().find_by_id(assignment, ex).await?.ok_or_else(conflict)?;
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
            participant: execution.assignee_participant_id.as_str() == event.evidence.recipient
                || execution
                    .additional_assignees
                    .iter()
                    .any(|p| p.participant_id.as_str() == event.evidence.recipient),
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
        for target in targets {
            let exists = match target.resource {
                ResourceKind::SalesOrder => {
                    self.db.sales_orders().find_by_id(&target.id, ex).await?.is_some()
                },
                ResourceKind::PurchaseOrder => {
                    self.db.purchase_orders().find_by_id(&target.id, ex).await?.is_some()
                },
            };
            if !exists {
                return Err(conflict());
            }
        }
        Ok(())
    }
    async fn validate_source(&self, update: &SourceUpdate, ex: &mut dyn Executor) -> Result<()> {
        let expected = self.compute(self.event.as_ref().ok_or_else(conflict)?, ex).await?;
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
