//! 为任务投影补齐处理人与提交人真实姓名。
//!
//! 队列页不得把「当前处理人」这种占位词上屏；姓名按账号主数据批量解析。
//! 简报里的「提交人」段由各 `*_brief.rs` 写入账号 ID（见 `sales_order_brief.rs`
//! 的 `submitted_by`），同样在这里解析；解析不到就整段摘掉——界面禁止展示账号 ID。

use std::collections::HashMap;

use database::{AccessControlExt, Executor, NoTransaction};
use mongodb::bson::doc;

use super::dto::WorkItemSummarySection;
use super::presentation::resolve_owner_display_name;
use super::{WorkItemService, WorkItemView};
use crate::errors::Result;

/// 简报里提交人段的标签，与 `brief.rs` 写入端保持一致。
const SUBMITTER_LABEL: &str = "提交人";

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
        let account_ids = account_ids_for_lookup(items);
        if account_ids.is_empty() {
            return Ok(());
        }
        let names = self.load_account_names(&account_ids, executor).await?;
        for item in items {
            if let Some(owner) = item.owner_user.as_mut() {
                owner.display_name = resolve_owner_display_name(&owner.id, &names);
            }
            apply_submitter_name(&mut item.summary_sections, &names);
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

/// 收集需要查姓名的账号 ID：处理人与简报提交人。
///
/// # 参数
/// * `items` - 任务投影
///
/// # 返回
/// 返回去重后的账号 ID；无处理人也无待解析提交人时为空。
///
/// # 错误
/// 无。
fn account_ids_for_lookup(items: &[WorkItemView]) -> Vec<String> {
    let mut ids = items
        .iter()
        .flat_map(|item| {
            let owner = item.owner_user.as_ref().map(|owner| owner.id.clone());
            let submitter = submitter_section(&item.summary_sections)
                .map(|section| section.value.clone())
                .filter(|value| is_account_id(value));
            owner.into_iter().chain(submitter)
        })
        .collect::<Vec<_>>();
    ids.sort();
    ids.dedup();
    ids
}

/// 找到简报里的提交人段。
///
/// # 参数
/// * `sections` - 简报键值段
///
/// # 返回
/// 存在「提交人」段时返回该段。
///
/// # 错误
/// 无。
fn submitter_section(sections: &[WorkItemSummarySection]) -> Option<&WorkItemSummarySection> {
    sections.iter().find(|section| section.label == SUBMITTER_LABEL)
}

/// 把简报提交人 ID 换成姓名；换不出姓名就摘掉该段。
///
/// # 参数
/// * `sections` - 简报键值段
/// * `names` - 账号 ID 到姓名
///
/// # 返回
/// 无。已经是姓名的段保持原样。
///
/// # 错误
/// 无。
fn apply_submitter_name(sections: &mut Vec<WorkItemSummarySection>, names: &HashMap<String, String>) {
    let Some(index) = sections
        .iter()
        .position(|section| section.label == SUBMITTER_LABEL)
    else {
        return;
    };
    if !is_account_id(&sections[index].value) {
        return;
    }
    match names
        .get(&sections[index].value)
        .map(String::as_str)
        .map(str::trim)
        .filter(|name| !name.is_empty())
    {
        Some(name) => sections[index].value = name.to_string(),
        None => {
            sections.remove(index);
        }
    }
}

/// 判断取值是否为账号稳定 ID：24 位 ObjectId 或本系统 32 位 hex。
///
/// # 参数
/// * `value` - 简报段取值
///
/// # 返回
/// 是账号 ID 时返回 `true`。
///
/// # 错误
/// 无。
fn is_account_id(value: &str) -> bool {
    let value = value.trim();
    matches!(value.len(), 24 | 32) && value.chars().all(|ch| ch.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::{account_ids_for_lookup, apply_submitter_name, is_account_id, WorkItemSummarySection};
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
            account_ids_for_lookup(&items),
            vec!["u1".to_string(), "u2".to_string()]
        );
    }

    const SUBMITTER_ID: &str = "7e9e521afce041b79218edb9a246e974";

    #[test]
    fn submitter_ids_join_the_same_account_lookup() {
        let mut item = dummy_view(Some("u1"));
        item.summary_sections = vec![section("提交人", SUBMITTER_ID)];
        assert_eq!(
            account_ids_for_lookup(&[item]),
            vec![SUBMITTER_ID.to_string(), "u1".to_string()]
        );
    }

    #[test]
    fn resolved_submitter_ids_become_names() {
        let mut sections = vec![section("客户", "北方商贸"), section("提交人", SUBMITTER_ID)];
        let names = HashMap::from([(SUBMITTER_ID.to_string(), "周航".to_string())]);
        apply_submitter_name(&mut sections, &names);
        assert_eq!(sections[1].value, "周航");
    }

    #[test]
    fn unresolved_submitter_ids_are_dropped_rather_than_shown() {
        let mut sections = vec![section("客户", "北方商贸"), section("提交人", SUBMITTER_ID)];
        apply_submitter_name(&mut sections, &HashMap::new());
        assert_eq!(sections.len(), 1);
        assert_eq!(sections[0].label, "客户");
    }

    #[test]
    fn submitter_names_already_resolved_are_left_alone() {
        let mut sections = vec![section("提交人", "周航")];
        apply_submitter_name(&mut sections, &HashMap::new());
        assert_eq!(sections[0].value, "周航");
    }

    #[test]
    fn account_ids_are_hex_of_object_id_or_system_length() {
        assert!(is_account_id(SUBMITTER_ID));
        assert!(is_account_id("507f1f77bcf86cd799439011"));
        assert!(!is_account_id("周航"));
        assert!(!is_account_id("HT-7456920203"));
    }

    fn section(label: &str, value: &str) -> WorkItemSummarySection {
        WorkItemSummarySection {
            label: label.to_string(),
            value: value.to_string(),
            numeric: None,
        }
    }

    fn dummy_view(owner_id: Option<&str>) -> crate::work_item::WorkItemView {
        use crate::work_item::dto::{ProcessingState, WorkItemView};
        use entities::work_item::{AssignmentSource, WorkItemPriority, WorkItemStatus, WorkItemType};
        WorkItemView {
            id: "wi".to_string(),
            work_item_type: WorkItemType::ImportBusinessConfirmation,
            handler_key: "import_business_confirmation".to_string(),
            destination_workspace_id: "W01".to_string(),
            route_context: None,
            approval_node_execution_id: None,
            status: WorkItemStatus::Open,
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
