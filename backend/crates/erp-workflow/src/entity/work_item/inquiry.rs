//! 询价任务的稳定责任行、需求版本和分派规则。

use std::collections::BTreeSet;

use erp_core::{Error, Result};
use serde::{Deserialize, Serialize};

use super::{WorkItem, WorkItemStatus, WorkItemType};

/// 一位采购负责的稳定需求行。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct InquiryTaskAssignment {
    pub owner_user_id: String,
    pub line_ids: Vec<String>,
}

/// 已校验完整覆盖、无重复的询价分派。
pub struct InquiryTaskAssignments(Vec<InquiryTaskAssignment>);

impl InquiryTaskAssignments {
    /// 规范化分派并验证全部需求行恰好有一位责任人。
    ///
    /// # 参数
    /// `assignments` 为采购责任，`line_ids` 为当前有效需求行。
    /// # 返回
    /// 可直接创建责任任务的分派集合。
    /// # 错误
    /// 空责任、重复责任人、未知、重复或遗漏需求行时拒绝。
    pub fn new(mut assignments: Vec<InquiryTaskAssignment>, line_ids: &[String]) -> Result<Self> {
        let expected: BTreeSet<_> = line_ids.iter().cloned().collect();
        if expected.is_empty() || expected.len() != line_ids.len() || assignments.is_empty() {
            return Err(Error::from("询价必须完整分派有效需求行"));
        }
        let mut owners = BTreeSet::new();
        let mut actual = BTreeSet::new();
        for item in &mut assignments {
            item.owner_user_id = item.owner_user_id.trim().to_string();
            if item.owner_user_id.is_empty()
                || !owners.insert(item.owner_user_id.clone())
                || item.line_ids.is_empty()
            {
                return Err(Error::from("询价采购责任人或责任行为空、重复"));
            }
            for id in &item.line_ids {
                if !expected.contains(id) || !actual.insert(id.clone()) {
                    return Err(Error::from("询价责任行不存在或重复分派"));
                }
            }
            item.line_ids.sort();
        }
        if actual != expected {
            return Err(Error::from("询价存在未分派的需求行"));
        }
        Ok(Self(assignments))
    }

    /// 取出已校验责任集合。
    ///
    /// # 参数
    /// 无。
    /// # 返回
    /// 一位采购一组稳定需求行。
    /// # 错误
    /// 不返回错误。
    pub fn into_values(self) -> Vec<InquiryTaskAssignment> {
        self.0
    }
}

/// 生成询价任务冻结的需求身份。
///
/// # 参数
/// `requirement_version` 为已提交需求版本。
/// # 返回
/// 与业务任务一并冻结的版本键。
/// # 错误
/// 不返回错误；零版本不应创建任务。
pub fn inquiry_subject_version(requirement_version: u32) -> String {
    format!("requirement-{requirement_version}")
}

impl WorkItem {
    /// 校验询价任务当前责任、需求版本及责任行仍有效。
    ///
    /// # 参数
    /// `actor_id` 为当前账号，`inquiry_id` 与 `requirement_version` 为当前需求，
    /// `line_ids` 为当前需求有效行。
    /// # 返回
    /// 身份、状态、版本与责任范围均匹配时返回 `true`。
    /// # 错误
    /// 不返回错误；缺失、改派、终态、过时或损坏的范围均失败关闭。
    pub fn owns_inquiry_task(
        &self,
        actor_id: &str,
        inquiry_id: &str,
        requirement_version: u32,
        line_ids: &[String],
    ) -> bool {
        self.status == WorkItemStatus::Open
            && self.inquiry_responsibility_matches(actor_id, inquiry_id, requirement_version, line_ids)
    }

    /// 校验本人完成的询价任务及其冻结需求范围，只授予历史只读资格。
    /// # 参数
    /// `actor_id` 为当前账号，其余为任务对应的历史需求身份与有效行。
    /// # 返回
    /// 本人完成且责任范围完整匹配时为真。
    /// # 错误
    /// 不返回错误；关闭、改派、删除及缺失完成证据均拒绝。
    pub fn owns_completed_inquiry(
        &self,
        actor_id: &str,
        inquiry_id: &str,
        requirement_version: u32,
        line_ids: &[String],
    ) -> bool {
        self.status == WorkItemStatus::Completed
            && self.completed_by.as_deref() == Some(actor_id)
            && self
                .completed_at
                .is_some_and(|at| u64::try_from(at.unix_secs()).is_ok_and(|at| at >= self.base.created_at))
            && self.inquiry_responsibility_matches(actor_id, inquiry_id, requirement_version, line_ids)
    }

    fn inquiry_responsibility_matches(
        &self,
        actor_id: &str,
        inquiry_id: &str,
        requirement_version: u32,
        line_ids: &[String],
    ) -> bool {
        !self.base.is_deleted()
            && self.work_item_type == WorkItemType::SalesInquiryProcessing
            && self.business_object_type == "sales_inquiry"
            && self.business_object_id == inquiry_id
            && self.subject_version == inquiry_subject_version(requirement_version)
            && self.owner_user_id.as_deref() == Some(actor_id)
            && self.responsibility_key().is_some_and(|key| !key.trim().is_empty())
            && !self.responsibility_scope_ids().is_empty()
            && self.responsibility_scope_ids().iter().all(|id| line_ids.contains(id))
    }
}

#[cfg(test)]
mod tests {
    use erp_core::common::time::Instant;
    use erp_core::ids::WorkItemId;

    use super::*;
    use crate::entity::work_item::{AssignmentSource, WorkItemData, WorkItemPriority};
    use crate::service::work_item::access::ensure_generic_work_item_mutation;

    #[test]
    fn assignments_require_partition_of_exact_current_lines() {
        let lines = vec!["a".into(), "b".into()];
        let assignment = |owner: &str, ids: &[&str]| InquiryTaskAssignment {
            owner_user_id: owner.into(),
            line_ids: ids.iter().map(|id| (*id).into()).collect(),
        };
        assert!(InquiryTaskAssignments::new(vec![assignment(" buyer ", &["b", "a"])], &lines).is_ok());
        for invalid in [
            vec![],
            vec![assignment("buyer", &["a"])],
            vec![assignment("buyer", &["a", "a"])],
            vec![assignment("buyer", &["a"]), assignment("buyer", &["b"])],
            vec![assignment("buyer", &["a", "c"])],
        ] {
            assert!(InquiryTaskAssignments::new(invalid, &lines).is_err());
        }
    }

    #[test]
    fn inquiry_grant_expires_with_owner_version_state_or_scope() {
        let mut item = WorkItem::new_with_responsibility_scope(
            WorkItemId::new("task"),
            WorkItemData {
                work_item_type: WorkItemType::SalesInquiryProcessing,
                business_object_type: "sales_inquiry".into(),
                business_object_id: "inquiry".into(),
                subject_version: inquiry_subject_version(2),
                owner_role: "inquiry_buyer".into(),
                owner_organization_id: "org".into(),
                owner_user_id: "buyer".into(),
                assignment_source: AssignmentSource::SystemRule,
                priority: WorkItemPriority::Normal,
                due_at: None,
                reason_code: None,
                impact_summary: None,
            },
            "inquiry:buyer",
            vec!["a".into()],
        )
        .unwrap();
        let lines = vec!["a".into(), "b".into()];
        assert!(item.owns_inquiry_task("buyer", "inquiry", 2, &lines));
        assert!(ensure_generic_work_item_mutation(&item).is_err());
        assert!(!item.owns_inquiry_task("other", "inquiry", 2, &lines));
        assert!(!item.owns_inquiry_task("buyer", "inquiry", 3, &lines));
        assert!(!item.owns_inquiry_task("buyer", "inquiry", 2, &["b".into()]));
        item.reassign("next", Instant::now()).unwrap();
        assert!(!item.owns_inquiry_task("buyer", "inquiry", 2, &lines));
        item.complete_by_domain_command("next", Instant::now()).unwrap();
        assert!(!item.owns_inquiry_task("next", "inquiry", 2, &lines));
        assert!(item.owns_completed_inquiry("next", "inquiry", 2, &lines));
        assert!(!item.owns_completed_inquiry("buyer", "inquiry", 2, &lines));
        assert!(!item.owns_completed_inquiry("next", "other", 2, &lines));
        assert!(!item.owns_completed_inquiry("next", "inquiry", 3, &lines));
        assert!(!item.owns_completed_inquiry("next", "inquiry", 2, &["b".into()]));
        item.status = WorkItemStatus::Closed;
        assert!(!item.owns_completed_inquiry("next", "inquiry", 2, &lines));
        item.status = WorkItemStatus::Completed;
        item.completed_by = Some("other".into());
        assert!(!item.owns_completed_inquiry("next", "inquiry", 2, &lines));
        item.completed_by = Some("next".into());
        item.completed_at = None;
        assert!(!item.owns_completed_inquiry("next", "inquiry", 2, &lines));
    }
}
