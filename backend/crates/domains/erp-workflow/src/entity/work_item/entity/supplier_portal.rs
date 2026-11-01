//! 供应商申请单人确认任务的固定身份及完成合同。

use erp_core::common::time::Instant;
use erp_core::ids::WorkItemId;
use erp_core::{Error, Result};

use super::{AssignmentSource, WorkItem, WorkItemCloseData, WorkItemData, WorkItemPriority, WorkItemType};

/// 供应商申请确认任务的服务端创建事实。
#[derive(Debug, Clone)]
pub struct SupplierPortalReviewTaskData {
    /// 申请稳定 ID。
    pub request_id: String,
    /// 本次冻结提交版本。
    pub subject_version: String,
    /// 已解析的内部采购确认人。
    pub owner_user_id: String,
    /// 当前确认人的责任组织。
    pub owner_organization_id: String,
    /// 确认时限。
    pub due_at: Option<Instant>,
    /// 内部确认工作面的业务摘要。
    pub impact_summary: Option<String>,
}

/// 调用方锁定的申请、提交版本及任务版本。
#[derive(Debug, Clone, Copy)]
pub struct SupplierPortalReviewIdentity<'a> {
    /// 待处理任务 ID。
    pub task_id: &'a str,
    /// 申请稳定 ID。
    pub request_id: &'a str,
    /// 冻结提交版本。
    pub subject_version: &'a str,
    /// 客户端确认的任务乐观锁版本。
    pub task_version: u64,
}

impl WorkItem {
    /// 构造指定到人的供应商申请确认任务。
    ///
    /// # 参数
    /// * `id` - 稳定任务 ID。
    /// * `data` - 已由提交过程解析的申请与责任事实。
    /// # 返回
    /// 返回冻结申请身份和责任键的开放任务。
    /// # 错误
    /// 必填字段为空、过长或任务基础数据非法时返回错误。
    pub fn new_supplier_portal_review(id: WorkItemId, data: SupplierPortalReviewTaskData) -> Result<Self> {
        let request_id = data.request_id.trim().to_string();
        let responsibility_key = format!("supplier_portal_request:{request_id}");
        Self::new_with_responsibility_key(
            id,
            WorkItemData {
                work_item_type: WorkItemType::SupplierPortalReview,
                business_object_type: "supplier_portal_request".into(),
                business_object_id: request_id,
                subject_version: data.subject_version,
                owner_user_id: data.owner_user_id,
                owner_role: "supplier_portal_reviewer".into(),
                owner_organization_id: data.owner_organization_id,
                assignment_source: AssignmentSource::SystemRule,
                priority: WorkItemPriority::Normal,
                due_at: data.due_at,
                reason_code: Some("SUPPLIER_PORTAL_REVIEW_REQUIRED".into()),
                impact_summary: data.impact_summary,
            },
            responsibility_key,
        )
    }

    /// 校验任务身份、申请身份、冻结提交及任务版本完全一致。
    ///
    /// # 参数
    /// * `identity` - 调用方锁定的确认身份。
    /// # 返回
    /// 完全一致时返回 `true`。
    /// # 错误
    /// 无；未知类型、审批引用或空版本失败关闭。
    pub fn matches_supplier_portal_review(&self, identity: SupplierPortalReviewIdentity<'_>) -> bool {
        self.work_item_type == WorkItemType::SupplierPortalReview
            && self.approval_node_execution_id.is_none()
            && self.base.id == identity.task_id
            && self.matches_business_object("supplier_portal_request", identity.request_id)
            && !identity.subject_version.trim().is_empty()
            && self.matches_subject_version(identity.subject_version)
            && identity.task_version > 0
            && self.base.version == identity.task_version
            && self.responsibility_key()
                == Some(format!("supplier_portal_request:{}", identity.request_id).as_str())
    }

    /// 随申请正式通过或退回决定完成确认任务。
    ///
    /// # 参数
    /// * `actor_id` - 本次内部决定人。
    /// * `at` - 本次决定时间。
    /// # 返回
    /// 完成后返回 `Ok(())`，保留全部责任历史。
    /// # 错误
    /// 类型错误、任务非开放或操作人不是当前责任人时返回错误。
    pub fn complete_supplier_portal_review(&mut self, actor_id: &str, at: Instant) -> Result<()> {
        self.ensure_supplier_portal_review()?;
        self.complete_open(actor_id.to_string(), at)?;
        self.last_activity_at = Some(at);
        Ok(())
    }

    /// 随供应商撤回申请关闭确认任务。
    ///
    /// # 参数
    /// * `actor_id` - 已由申请过程校验的供应商申请人。
    /// * `reason` - 申请撤回原因。
    /// * `at` - 撤回时间。
    /// # 返回
    /// 返回不可逆的关闭任务；不形成内部采购确认结果。
    /// # 错误
    /// 类型错误、任务非开放或关闭事实非法时返回错误。
    pub fn withdraw_supplier_portal_review(
        &mut self,
        actor_id: &str,
        reason: &str,
        at: Instant,
    ) -> Result<()> {
        self.ensure_supplier_portal_review()?;
        self.close_open(actor_id.to_string(), WorkItemCloseData { close_reason: reason.into() }, at)
    }

    /// 限定专用生命周期命令只能作用于供应商申请确认任务。
    fn ensure_supplier_portal_review(&self) -> Result<()> {
        if self.work_item_type != WorkItemType::SupplierPortalReview
            || self.business_object_type != "supplier_portal_request"
            || self.approval_node_execution_id.is_some()
        {
            return Err(Error::from("任务不是供应商申请确认任务"));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::super::direct_data;
    use super::*;
    use crate::entity::work_item::WorkItemStatus;

    /// 构造一份冻结版本的单人确认任务。
    fn task() -> WorkItem {
        WorkItem::new_supplier_portal_review(
            WorkItemId::new("task"),
            SupplierPortalReviewTaskData {
                request_id: "request".into(),
                subject_version: "submission-1".into(),
                owner_user_id: "reviewer".into(),
                owner_organization_id: "procurement".into(),
                due_at: None,
                impact_summary: Some("供给报价待确认".into()),
            },
        )
        .unwrap()
    }

    /// 返回与测试任务精确匹配的命令身份。
    fn identity() -> SupplierPortalReviewIdentity<'static> {
        SupplierPortalReviewIdentity {
            task_id: "task",
            request_id: "request",
            subject_version: "submission-1",
            task_version: 1,
        }
    }

    #[test]
    fn review_identity_rejects_wrong_objects_and_stale_versions() {
        let item = task();
        assert!(item.matches_supplier_portal_review(identity()));
        for mismatched in [
            SupplierPortalReviewIdentity { request_id: "other", ..identity() },
            SupplierPortalReviewIdentity { task_id: "other", ..identity() },
            SupplierPortalReviewIdentity { subject_version: "submission-2", ..identity() },
            SupplierPortalReviewIdentity { task_version: 0, ..identity() },
            SupplierPortalReviewIdentity { task_version: 2, ..identity() },
        ] {
            assert!(!item.matches_supplier_portal_review(mismatched));
        }
        assert_eq!(item.owner_user_id.as_deref(), Some("reviewer"));
        assert!(!item.is_w29_closable());
    }

    #[test]
    fn only_current_internal_reviewer_can_complete_after_reassignment() {
        let mut item = task();
        assert!(item.complete_by_domain_command("reviewer", Instant::from_unix_secs(100)).is_err());
        assert!(item.complete_supplier_portal_review("other", Instant::from_unix_secs(100)).is_err());
        item.reassign("replacement", Instant::from_unix_secs(110)).unwrap();
        assert!(item.complete_supplier_portal_review("reviewer", Instant::from_unix_secs(120)).is_err());
        item.complete_supplier_portal_review("replacement", Instant::from_unix_secs(120)).unwrap();
        assert_eq!(item.status, WorkItemStatus::Completed);
        assert_eq!(item.completed_by.as_deref(), Some("replacement"));
        assert_eq!(item.responsibility_actor_ids, ["reviewer", "replacement"]);
        assert!(
            item.withdraw_supplier_portal_review("supplier", "撤回", Instant::from_unix_secs(130)).is_err()
        );
    }

    #[test]
    fn withdrawal_keeps_supplier_actor_and_prevents_later_decision() {
        let mut item = task();
        let at = Instant::from_unix_secs(100);
        assert!(item.close("manager", WorkItemCloseData { close_reason: "清理".into() }, at).is_err());
        assert!(item.withdraw_supplier_portal_review("supplier", " ", at).is_err());
        item.withdraw_supplier_portal_review("supplier", "重新核对报价", at).unwrap();
        assert_eq!(item.status, WorkItemStatus::Closed);
        assert_eq!(item.closed_by.as_deref(), Some("supplier"));
        assert!(item.completed_by.is_none());
        assert!(item.complete_supplier_portal_review("reviewer", at).is_err());
        let mut unrelated = WorkItem::new(WorkItemId::new("other"), direct_data()).unwrap();
        assert!(unrelated.complete_supplier_portal_review("alice", at).is_err());
    }
}
