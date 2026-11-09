//! 本人待办按持久化分配读取；对象事实只补充展示，不参与可见性判定。

use std::collections::HashMap;

use erp_workflow::entity::work_item::WorkItemType;
use erp_workflow::{WorkItemExt, WorkItemRow};
use persistence_core::Executor;

use super::dto::WorkItemFields;
use super::facts::{apply_object_display, object_policy};
use super::{WorkbenchObjectFactMap, WorkbenchReadService};
use crate::Result;

impl WorkbenchReadService {
    /// 读取本人开放任务，供给任务保留冻结责任行；缺失业务对象不移除任务。
    /// # 参数
    /// `rows` 为责任筛选后的候选；`actor_id` 为当前账号；`facts` 仅用于展示；`executor` 沿用读取快照。
    /// # 返回
    /// 按原顺序返回当前分配的任务，不检查来源单据权限或办理资格。
    /// # 错误
    /// 供给任务责任行读取失败时返回错误。
    pub(super) async fn personal_queue_fields(
        &self,
        rows: Vec<WorkItemRow>,
        actor_id: &str,
        facts: &WorkbenchObjectFactMap,
        executor: &mut dyn Executor,
    ) -> Result<Vec<WorkItemFields>> {
        let mut fields = rows
            .into_iter()
            .map(WorkItemFields::from)
            .filter(|item| item.is_pending_for(actor_id))
            .collect::<Vec<_>>();
        let ids = fields
            .iter()
            .filter(|item| item.work_item_type == WorkItemType::ProcurementOrderCreation)
            .map(|item| item.id.clone())
            .collect::<Vec<_>>();
        let tasks = self
            .db
            .work_items()
            .list_active_by_ids(&ids, executor)
            .await?
            .into_iter()
            .map(|item| (item.base.id.clone(), item))
            .collect::<HashMap<_, _>>();
        for field in &mut fields {
            if let Some(task) = tasks.get(&field.id).filter(|task| task.base.version == field.task_version) {
                field.procurement_scope_ids = task.responsibility_scope_ids().to_vec();
            }
            if let Some(policy) = object_policy(field.work_item_type, &field.business_object_type)
                && let Some(fact) = facts.get(&(policy.object_kind, field.business_object_id.clone()))
            {
                apply_object_display(field, fact);
            }
        }
        Ok(fields)
    }
}
