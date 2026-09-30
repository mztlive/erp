//! 审批队列按精确任务责任授权，与普通业务对象范围隔离。
use std::collections::HashMap;

use application_core::AuditActor;
use erp_workflow::service::approval::execution::runtime_service::approval_task_readable_with_executor;
use erp_workflow::{WorkItem, WorkItemExt, WorkItemRow, WorkflowAuthorizationPort};
use persistence_core::Executor;

use super::access::{ActorAccess, authorized_fields};
use super::dto::WorkItemFields;
use super::facts::{apply_object_display, object_policy};
use super::{WorkbenchObjectFactMap, WorkbenchReadService};
use crate::Result;

impl<A: WorkflowAuthorizationPort + Clone + Send + Sync + 'static> WorkbenchReadService<A> {
    /// 审批读取先证明任务链，其他任务仍执行原业务范围过滤。
    pub(super) async fn authorize_queue_rows(
        &self,
        rows: Vec<WorkItemRow>,
        access: &ActorAccess,
        facts: &mut WorkbenchObjectFactMap,
        executor: &mut dyn Executor,
    ) -> Result<Vec<WorkItemFields>> {
        let original = facts.clone();
        self.filter_order_access_keeping_owned_fulfillment(
            &access.actor_id,
            rows.iter().map(super::query::owned_fulfillment_task),
            facts,
            executor,
        )
        .await?;
        let ids = rows
            .iter()
            .filter(|row| row.work_item_type.is_document_approval())
            .map(|row| row.id.clone())
            .collect::<Vec<_>>();
        let tasks = self
            .db
            .work_items()
            .list_active_by_ids(&ids, executor)
            .await?
            .into_iter()
            .map(|item| (item.base.id.clone(), item))
            .collect::<HashMap<_, _>>();
        let ordinary_facts = facts.clone();
        let mut result = Vec::new();
        for row in rows {
            if !row.work_item_type.is_document_approval() {
                result.extend(authorized_fields(vec![row], access, &ordinary_facts));
                continue;
            }
            let Some(item) = tasks.get(&row.id).filter(|item| item.base.version == row.version) else {
                continue;
            };
            if let Some(fields) = self.authorize_approval_item(item, access, &original, executor).await? {
                if let Some(policy) = object_policy(item.work_item_type, &item.business_object_type) {
                    let key = (policy.object_kind, item.business_object_id.clone());
                    if let Some(fact) = original.get(&key) {
                        facts.insert(key, fact.clone());
                    }
                }
                result.push(fields);
            }
        }
        Ok(result)
    }

    /// 已证明的审批任务只装配其对应提交版本，不要求普通销售或采购详情范围。
    pub(super) async fn authorize_approval_item(
        &self,
        item: &WorkItem,
        access: &ActorAccess,
        facts: &WorkbenchObjectFactMap,
        executor: &mut dyn Executor,
    ) -> Result<Option<WorkItemFields>> {
        let Some(account) = self.auth.load_account(&access.actor_id, executor).await? else {
            return Ok(None);
        };
        let actor = AuditActor::new(account.id, account.login_account, account.kind);
        if !approval_task_readable_with_executor(&self.db, &self.auth, &actor, item, executor).await? {
            return Ok(None);
        }
        let Some(policy) = object_policy(item.work_item_type, &item.business_object_type) else {
            return Ok(None);
        };
        let Some(fact) = facts.get(&(policy.object_kind, item.business_object_id.clone())) else {
            return Ok(None);
        };
        let mut fields = WorkItemFields::from(item.clone());
        apply_object_display(&mut fields, fact);
        Ok(Some(fields))
    }
}
