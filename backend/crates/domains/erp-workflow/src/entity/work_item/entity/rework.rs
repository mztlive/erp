//! 审批终态后的原提交人处理责任。
use erp_core::common::time::Instant;
use erp_core::{Error, Result};

use super::{WorkItem, WorkItemCloseData, WorkItemType};

impl WorkItem {
    /// 原提交人放弃继续处理本次驳回，保留单据及历史。
    /// # 参数
    /// `actor` 为原提交人，`reason` 为必填说明，`now` 为操作时间。
    /// # 返回
    /// 任务关闭；后续业务重提仍可创建新审批实例。
    /// # 错误
    /// 类型、责任人、状态或原因不合法时拒绝。
    pub fn abandon_approval_rework(&mut self, actor: &str, reason: &str, now: Instant) -> Result<()> {
        self.ensure_approval_rework()?;
        self.ensure_current_owner(actor)?;
        self.close_open(actor, WorkItemCloseData { close_reason: reason.to_owned() }, now)
    }

    /// 业务重提或作废已成功时结束驳回处理责任。
    /// # 参数
    /// `actor` 为已由业务命令授权的操作人，`now` 为业务时间。
    /// # 返回
    /// 开放任务标为已完成并保留原提交人责任历史。
    /// # 错误
    /// 非驳回处理任务、缺失实例关联或任务已结束时拒绝。
    pub fn complete_approval_rework(&mut self, actor: &str, now: Instant) -> Result<()> {
        self.ensure_approval_rework()?;
        if actor.trim().is_empty() {
            return Err(Error::from("驳回处理完成人不能为空"));
        }
        self.complete_open_by_system(now)?;
        self.completed_by = Some(actor.to_owned());
        Ok(())
    }

    /// 校验驳回任务绑定固定实例和原提交人。
    pub(super) fn ensure_approval_rework(&self) -> Result<()> {
        if self.work_item_type != WorkItemType::ApprovalRework
            || self.approval_node_execution_id.is_some()
            || !self
                .responsibility_key()
                .is_some_and(|key| key.starts_with("approval_rework:") && key.len() > 16)
            || self.owner_role != "approval_submitter"
            || self.work_item_type.brief_relation(&self.business_object_type).is_none()
        {
            return Err(Error::from("任务不是有效的驳回待处理责任"));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use erp_core::ids::WorkItemId;

    use super::*;
    use crate::entity::work_item::{AssignmentSource, WorkItemData, WorkItemPriority, WorkItemStatus};

    fn task() -> WorkItem {
        WorkItem::new_with_responsibility_key(
            WorkItemId::new("rework"),
            WorkItemData {
                work_item_type: WorkItemType::ApprovalRework,
                business_object_type: "sales_order".into(),
                business_object_id: "order".into(),
                subject_version: "1".into(),
                owner_role: "approval_submitter".into(),
                owner_organization_id: "org".into(),
                owner_user_id: "submitter".into(),
                assignment_source: AssignmentSource::SystemRule,
                priority: WorkItemPriority::Normal,
                due_at: None,
                reason_code: Some("APPROVAL_REJECTED".into()),
                impact_summary: None,
            },
            "approval_rework:instance",
        )
        .unwrap()
    }

    #[test]
    fn only_submitter_can_abandon_and_generic_mutations_cannot_override_responsibility() {
        let mut item = task();
        let before = item.clone();
        let now = Instant::from_unix_secs(100);
        assert!(item.abandon_approval_rework("other", "放弃", now).is_err());
        assert!(item.abandon_approval_rework("submitter", " ", now).is_err());
        assert!(item.reassign("other", now).is_err());
        assert!(item.complete_by_domain_command("submitter", now).is_err());
        assert_eq!(item, before);
        item.abandon_approval_rework("submitter", "不再办理", now).unwrap();
        assert_eq!(item.status, WorkItemStatus::Closed);
        assert_eq!(item.close_reason.as_deref(), Some("不再办理"));
        assert!(item.complete_approval_rework("submitter", now).is_err());
    }

    #[test]
    fn resubmission_completes_rework_without_losing_original_responsibility() {
        let mut item = task();
        item.complete_approval_rework("submitter", Instant::from_unix_secs(100)).unwrap();
        assert_eq!(item.status, WorkItemStatus::Completed);
        assert_eq!(item.owner_user_id.as_deref(), Some("submitter"));
        assert_eq!(item.responsibility_actor_ids, vec!["submitter"]);
    }
}
