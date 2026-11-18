//! 回执查证、真实任务及销售来源授权共用原快照；查证不重放写入。
use std::future::Future;
use std::sync::Arc;

use application_core::command_recovery::recover_resource;
use application_core::{AuditActor, CommandReceipt};
use async_trait::async_trait;
use erp_fulfillment::FulfillmentCommandReceiptService;
use erp_fulfillment::dto::CommitCustomerAcceptanceRequest;
use erp_fulfillment::entity::fulfillment::{CustomerAcceptance, CustomerAcceptanceState};
use erp_fulfillment::repository::FulfillmentExt;
use erp_identity::entity::authorization::{FactValue, ObjectFacts};
use erp_identity::{AccessTarget, AuthorizationService};
use erp_permissions::{Permission, fulfillment, internal, sales_order};
use erp_read_models::sales_center::access::SalesAccess;
use erp_workflow::repository::prelude::*;
use erp_workflow::{WorkItem, WorkItemExt, WorkItemType};
use mongodb::Database;
use persistence_core::Executor;

use super::super::task_access::{TaskPurpose, require_assignee};
use super::task::responsibility_key;
use crate::{Error, Result};

#[async_trait]
pub(super) trait AcceptanceReplay: Sync {
    async fn operation(
        &self,
        actor: &AuditActor,
        permission: &Permission,
        executor: &mut dyn Executor,
    ) -> Result<()>;
    async fn receipt(&self, receipt: &CommandReceipt, executor: &mut dyn Executor) -> Result<Option<String>>;
    async fn acceptance(&self, id: &str, executor: &mut dyn Executor) -> Result<CustomerAcceptance>;
    async fn sales(&self, actor: &AuditActor, id: &str, executor: &mut dyn Executor) -> Result<()>;
    async fn task(
        &self,
        actor: &AuditActor,
        sales: &str,
        task: Option<&str>,
        executor: &mut dyn Executor,
    ) -> Result<()>;
}

pub(super) struct ReplayReader {
    pub db: Database,
    pub authorization: Arc<AuthorizationService>,
}

#[async_trait]
impl AcceptanceReplay for ReplayReader {
    async fn operation(
        &self,
        actor: &AuditActor,
        permission: &Permission,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        if permission == &fulfillment::CUSTOMER_ACCEPTANCE_CUSTOMER_ACCEPTANCE_COMMIT {
            let candidate = ObjectFacts {
                resource: "work_item".into(),
                subject_type: FactValue::Known(WorkItemType::CustomerAcceptanceRegistration.as_str().into()),
                subject_source_resource: FactValue::Known("sales_order".into()),
                ..Default::default()
            };
            self.authorization
                .require(
                    actor,
                    &internal::WORK_ITEM_EXECUTION_QUALIFICATION,
                    AccessTarget::Candidate(candidate),
                    executor,
                )
                .await?;
        }
        self.authorization.require(actor, permission, AccessTarget::Operation, executor).await?;
        Ok(())
    }
    async fn receipt(&self, receipt: &CommandReceipt, executor: &mut dyn Executor) -> Result<Option<String>> {
        Ok(FulfillmentCommandReceiptService::new(self.db.clone())
            .committed_resource_id(receipt, executor)
            .await?)
    }
    async fn acceptance(&self, id: &str, executor: &mut dyn Executor) -> Result<CustomerAcceptance> {
        self.db
            .customer_acceptances()
            .find_by_id(id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("验收单不存在".into()))
    }
    async fn sales(&self, actor: &AuditActor, id: &str, executor: &mut dyn Executor) -> Result<()> {
        SalesAccess::new(self.db.clone(), self.authorization.clone())
            .require_object(actor, &sales_order::DETAIL, id, executor)
            .await?;
        Ok(())
    }
    async fn task(
        &self,
        actor: &AuditActor,
        sales: &str,
        task_id: Option<&str>,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let task = self.result_task(sales, task_id, executor).await?;
        require_assignee(&self.authorization, actor, &task, TaskPurpose::CustomerAcceptance, executor).await
    }
}

impl ReplayReader {
    // 有明确任务时读取当前持久化版本；旧请求只接受唯一真实验收任务。
    async fn result_task(
        &self,
        sales: &str,
        task_id: Option<&str>,
        executor: &mut dyn Executor,
    ) -> Result<WorkItem> {
        let tasks = if let Some(id) = task_id {
            self.db.work_items().find_work_item(id, executor).await?.into_iter().collect()
        } else {
            self.db
                .work_items()
                .customer_acceptance_result_candidates(sales, &responsibility_key(sales), executor)
                .await?
        };
        let [task] = tasks.as_slice() else {
            return Err(Error::ConflictError("无法确认原验收登记对应的待办，请先核对验收记录".into()));
        };
        if task.work_item_type != WorkItemType::CustomerAcceptanceRegistration
            || !task.matches_business_object("sales_order", sales)
            || task.responsibility_key() != Some(responsibility_key(sales).as_str())
        {
            return Err(Error::ConflictError("原验收结果与验收待办不符，请核对待办中的验收记录".into()));
        }
        Ok(task.clone())
    }
}

// 回放使用当前真实任务版本，不用已经消费的输入任务版本重新办理。
pub(super) async fn committed(
    reader: &impl AcceptanceReplay,
    req: &CommitCustomerAcceptanceRequest,
    actor: &AuditActor,
    receipt: &CommandReceipt,
    executor: &mut dyn Executor,
) -> Result<Option<CustomerAcceptance>> {
    snapshot(executor)?;
    reader.operation(actor, &fulfillment::CUSTOMER_ACCEPTANCE_CUSTOMER_ACCEPTANCE_COMMIT, executor).await?;

    let Some(id) = reader.receipt(receipt, executor).await? else { return Ok(None) };
    let acceptance = reader.acceptance(&id, executor).await?;
    if acceptance.sales_order_id != req.sales_order_id
        || req.acceptance_id.as_ref().is_some_and(|id| id != &acceptance.base.id)
        || acceptance.reversal_of_acceptance_id.is_some()
        || !matches!(acceptance.status, CustomerAcceptanceState::Posted | CustomerAcceptanceState::Reversed)
    {
        return Err(Error::ConflictError("原验收记录与当前销售单不符，请核对所验收的订单".into()));
    }

    reader.sales(actor, acceptance.sales_order_id.as_ref(), executor).await?;
    reader.task(actor, acceptance.sales_order_id.as_ref(), req.work_item_id.as_deref(), executor).await?;
    Ok(Some(acceptance))
}

// 冲正功能资格先于原单及回执读取；结果必须仍指向原单的真实销售来源。
pub(super) async fn reversed(
    reader: &impl AcceptanceReplay,
    id: &str,
    actor: &AuditActor,
    receipt: &CommandReceipt,
    executor: &mut dyn Executor,
) -> Result<Option<CustomerAcceptance>> {
    snapshot(executor)?;
    reader.operation(actor, &fulfillment::CUSTOMER_ACCEPTANCE_CUSTOMER_ACCEPTANCE_REVERSE, executor).await?;

    let original = reader.acceptance(id, executor).await?;
    reader.sales(actor, original.sales_order_id.as_ref(), executor).await?;

    let Some(result) = reader.receipt(receipt, executor).await? else { return Ok(None) };
    let result = reader.acceptance(&result, executor).await?;
    if result.status != CustomerAcceptanceState::Posted
        || result.reversal_of_acceptance_id.as_ref().map(|id| id.as_ref()) != Some(id)
        || result.sales_order_id != original.sales_order_id
    {
        return Err(Error::ConflictError("原验收冲正记录与验收单或销售单不符，请核对原单".into()));
    }
    Ok(Some(result))
}

// 确定失败直接返回；未知提交仅查证持久化结果，查证失败仍保留原错误。
pub(super) async fn recover<T>(error: Error, lookup: impl Future<Output = Result<Option<T>>>) -> Result<T> {
    if !matches!(error, Error::OutcomeUnknown(_)) {
        return Err(error);
    }
    recover_resource(error, lookup.await, |error| matches!(error, Error::OutcomeUnknown(_)))
}

fn snapshot(executor: &mut dyn Executor) -> Result<()> {
    if executor.boundary().is_some_and(|boundary| boundary.is_snapshot() && boundary.is_current(executor)) {
        Ok(())
    } else {
        Err(Error::ValidationError("验收命令回放必须使用原授权快照".into()))
    }
}

#[cfg(test)]
mod tests;
