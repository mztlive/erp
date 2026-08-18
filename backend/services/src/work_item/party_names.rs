//! 为任务投影补齐处理人真实姓名。
//!
//! 队列页不得把「当前处理人」这种占位词上屏；姓名按账号主数据批量解析。

use std::collections::HashMap;

use database::{AccessControlExt, Executor, NoTransaction};
use mongodb::bson::doc;

use super::presentation::resolve_owner_display_name;
use super::{WorkItemService, WorkItemView};
use crate::errors::Result;

impl WorkItemService {
    /// 把投影中的处理人占位名替换为账号姓名。
    ///
    /// # 参数
    /// * `items` - 待补齐姓名的任务投影
    ///
    /// # 返回
    /// 成功时就地更新 `owner_user.display_name`。
    ///
    /// # 错误
    /// 账号查询失败时返回仓储错误。
    pub(super) async fn apply_party_names(&self, items: &mut [WorkItemView]) -> Result<()> {
        self.apply_party_names_with(&mut NoTransaction, items).await
    }

    /// 在指定执行器上批量解析处理人姓名。
    ///
    /// # 参数
    /// * `executor` - 数据访问执行器
    /// * `items` - 待补齐姓名的任务投影
    ///
    /// # 返回
    /// 成功时就地更新展示名；没有处理人时不查询。
    ///
    /// # 错误
    /// 账号查询失败时返回仓储错误。
    async fn apply_party_names_with(
        &self,
        executor: &mut dyn Executor,
        items: &mut [WorkItemView],
    ) -> Result<()> {
        let owner_ids = owner_ids_for_lookup(items);
        if owner_ids.is_empty() {
            return Ok(());
        }
        let names = self.load_account_names(&owner_ids, executor).await?;
        for item in items {
            let Some(owner) = item.owner_user.as_mut() else {
                continue;
            };
            owner.display_name = resolve_owner_display_name(&owner.id, &names);
        }
        Ok(())
    }

    /// 按账号 ID 批量读取姓名。
    ///
    /// # 参数
    /// * `owner_ids` - 去重后的账号 ID
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回账号 ID 到姓名的映射。
    ///
    /// # 错误
    /// 账号查询失败时返回仓储错误。
    async fn load_account_names(
        &self,
        owner_ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, String>> {
        Ok(self
            .db
            .accounts()
            .find_many(doc! { "id": { "$in": owner_ids } }, executor)
            .await?
            .into_iter()
            .map(|account| (account.base.id, account.name))
            .collect())
    }
}

/// 收集需要查姓名的处理人 ID。
///
/// # 参数
/// * `items` - 任务投影
///
/// # 返回
/// 返回去重后的账号 ID；无处理人时为空。
///
/// # 错误
/// 无。
fn owner_ids_for_lookup(items: &[WorkItemView]) -> Vec<String> {
    let mut ids = items
        .iter()
        .filter_map(|item| item.owner_user.as_ref().map(|owner| owner.id.clone()))
        .collect::<Vec<_>>();
    ids.sort();
    ids.dedup();
    ids
}

#[cfg(test)]
mod tests {
    use super::owner_ids_for_lookup;
    use crate::work_item::dto::WorkItemPartyView;

    #[test]
    fn owner_ids_are_unique_and_skip_unassigned() {
        let items = [
            dummy_view(Some("u2")),
            dummy_view(Some("u1")),
            dummy_view(Some("u2")),
            dummy_view(None),
        ];
        assert_eq!(
            owner_ids_for_lookup(&items),
            vec!["u1".to_string(), "u2".to_string()]
        );
    }

    fn dummy_view(owner_id: Option<&str>) -> crate::work_item::WorkItemView {
        use crate::work_item::dto::{ProcessingState, WorkItemView};
        use entities::work_item::{
            AssignmentMode, AssignmentSource, WorkItemPriority, WorkItemStatus, WorkItemType,
        };
        WorkItemView {
            id: "wi".to_string(),
            work_item_type: WorkItemType::ProcurementConfirmation,
            handler_key: "procurement_confirmation".to_string(),
            destination_workspace_id: "W07".to_string(),
            route_context: None,
            approval_step_instance_id: None,
            approval_node_execution_id: None,
            status: WorkItemStatus::Open,
            assignment_mode: AssignmentMode::Direct,
            assignment_source: AssignmentSource::SystemRule,
            owner_role: "role-procurement".to_string(),
            owner_role_label: "采购".to_string(),
            owner_organization_id: "company".to_string(),
            owner_organization: WorkItemPartyView {
                id: "company".to_string(),
                display_name: "责任组织".to_string(),
            },
            owner_user_id: owner_id.map(str::to_string),
            owner_user: owner_id.map(|id| WorkItemPartyView {
                id: id.to_string(),
                display_name: "当前处理人".to_string(),
            }),
            processing_state: ProcessingState::Ready,
            processing_blocker: None,
            business_object_type: "procurement_confirmation".to_string(),
            business_object_id: "pc-1".to_string(),
            root_business_object_id: "so-1".to_string(),
            business_object_label: "销售单 SO-1".to_string(),
            counterparty_label: None,
            next_action_hint: "进入采购确认页后，逐行确认可供数量；确认通过后销售单才会生效。".to_string(),
            summary_sections: Vec::new(),
            brief_lines: Vec::new(),
            brief_more_count: None,
            list_summary: None,
            subject_version: "1".to_string(),
            task_version: "1".to_string(),
            allowed_actions: Vec::new(),
            action_blockers: Vec::new(),
            priority: WorkItemPriority::Normal,
            due_at: None,
            reason_code: None,
            reason_label: "销售已提交，需要采购确认能否供货".to_string(),
            impact_summary: "不确认则销售单不能生效".to_string(),
            assigned_at: None,
            started_at: None,
            current_assignment_at: None,
            last_activity_at: None,
            completed_at: None,
            completed_by: None,
            closed_at: None,
            closed_by: None,
            close_reason: None,
            created_at: 1,
            queue_context_id: "qc".to_string(),
        }
    }
}
