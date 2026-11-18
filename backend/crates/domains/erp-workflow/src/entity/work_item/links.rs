//! 任务绑定业务版本的关联链路；查询只消费持久化引用，不追查业务单据。

use erp_core::validation::normalize_required_text;
use erp_core::{Error, Result};
use serde::{Deserialize, Serialize};

use super::WorkItem;
use crate::entity::approval_integration::ApprovalSubjectSnapshot;

/// 任务直接对象或上级对象的稳定引用；类型和 ID 必须成对匹配。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(try_from = "LinkData")]
pub struct WorkItemLink {
    object_type: String,
    object_id: String,
}

#[derive(Deserialize)]
struct LinkData {
    object_type: String,
    object_id: String,
}

impl TryFrom<LinkData> for WorkItemLink {
    type Error = Error;

    fn try_from(value: LinkData) -> Result<Self> {
        Self::new(value.object_type, value.object_id)
    }
}

impl WorkItemLink {
    /// 构造经过规范化的对象引用。
    /// # 参数
    /// `object_type` 为稳定对象类型，`object_id` 为该类型下的稳定 ID。
    /// # 返回
    /// 返回可排序、去重的类型化引用。
    /// # 错误
    /// 类型或 ID 为空、超长时拒绝。
    pub fn new(object_type: impl Into<String>, object_id: impl Into<String>) -> Result<Self> {
        Ok(Self {
            object_type: normalize_required_text(
                object_type.into(),
                "关联对象类型不能为空",
                128,
                "关联对象类型过长",
            )?,
            object_id: normalize_required_text(
                object_id.into(),
                "关联对象ID不能为空",
                128,
                "关联对象ID过长",
            )?,
        })
    }

    /// 返回关联对象类型。
    /// # 参数
    /// 无。
    /// # 返回
    /// 返回稳定类型代码。
    /// # 错误
    /// 无。
    pub fn object_type(&self) -> &str {
        &self.object_type
    }

    /// 返回关联对象身份。
    /// # 参数
    /// 无。
    /// # 返回
    /// 返回该类型下的稳定 ID。
    /// # 错误
    /// 无。
    pub fn object_id(&self) -> &str {
        &self.object_id
    }
}

impl WorkItem {
    /// 返回任务对应业务版本的全部关联引用。
    /// # 参数
    /// 无。
    /// # 返回
    /// 返回直接对象及已写入的上级链路；旧数据未回填时为空。
    /// # 错误
    /// 无。
    pub fn links(&self) -> &[WorkItemLink] {
        &self.links
    }

    /// 在任务创建或开放任务业务关系变更时更新完整链路。
    /// # 参数
    /// `links` 为同一业务版本已确认的上级引用；直接对象由实体补齐。
    /// # 返回
    /// 原地保存排序、去重后的完整链路。
    /// # 错误
    /// 终态任务不能改写历史链路；引用超过 1000 个时拒绝。
    pub fn set_links(&mut self, mut links: Vec<WorkItemLink>) -> Result<()> {
        if self.status != super::WorkItemStatus::Open {
            return Err(Error::from("终态任务不能修改关联链路"));
        }

        links.push(WorkItemLink::new(&self.business_object_type, &self.business_object_id)?);
        links.sort();
        links.dedup();
        if links.len() > 1000 {
            return Err(Error::from("任务关联对象最多1000个"));
        }

        self.links = links;
        Ok(())
    }

    /// 继承与任务对象和提交版本一致的不可变审批链路。
    /// # 参数
    /// `snapshot` 为任务所属审批实例的冻结业务快照。
    /// # 返回
    /// 写入冻结链路，同时补齐任务直接对象。
    /// # 错误
    /// 对象类型、对象 ID、提交版本不一致或任务已结束时拒绝。
    pub fn inherit_approval_links(&mut self, snapshot: &ApprovalSubjectSnapshot) -> Result<()> {
        if !self.matches_business_object(snapshot.document_type.as_str(), &snapshot.business_object_id)
            || self.subject_version != snapshot.subject_version.to_string()
        {
            return Err(Error::from("任务关联链路与审批冻结对象或版本不一致"));
        }
        self.set_links(snapshot.links.clone())
    }

    /// 为尚未持久化的任务追加一个已确认的上级对象。
    /// # 参数
    /// `object_type` 和 `object_id` 为同一业务版本的上级引用。
    /// # 返回
    /// 返回追加并去重后的任务。
    /// # 错误
    /// 引用无效、任务终态或链路超限时拒绝。
    pub fn with_link(mut self, object_type: &str, object_id: impl Into<String>) -> Result<Self> {
        let mut links = self.links.clone();
        links.push(WorkItemLink::new(object_type, object_id)?);
        self.set_links(links)?;
        Ok(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn link_identity_includes_type_and_rejects_empty_values() {
        let sale = WorkItemLink::new(" sales_order ", " same ").unwrap();
        assert_eq!(sale.object_id(), "same");
        assert_ne!(sale, WorkItemLink::new("purchase_order", "same").unwrap());
        assert!(WorkItemLink::new("", "id").is_err());
        assert!(
            serde_json::from_str::<WorkItemLink>(r#"{"object_type":"sales_order","object_id":" "}"#).is_err()
        );
    }
    use erp_core::ids::WorkItemId;

    use crate::entity::work_item::{
        AssignmentSource, WorkItemData, WorkItemPriority, WorkItemStatus, WorkItemType,
    };

    fn task() -> WorkItem {
        WorkItem::new(
            WorkItemId::new("task"),
            WorkItemData {
                work_item_type: WorkItemType::CardFundsReview,
                business_object_type: "receivable_account".into(),
                business_object_id: "account".into(),
                subject_version: "1".into(),
                owner_role: "finance".into(),
                owner_organization_id: "org".into(),
                owner_user_id: "user".into(),
                assignment_source: AssignmentSource::SystemRule,
                priority: WorkItemPriority::Normal,
                due_at: None,
                reason_code: None,
                impact_summary: None,
            },
        )
        .unwrap()
    }

    #[test]
    fn replacing_links_preserves_direct_object_deduplicates_and_freezes_terminal_history() {
        let mut item = task();
        let sales = WorkItemLink::new("sales_order", "same").unwrap();
        let purchase = WorkItemLink::new("purchase_order", "same").unwrap();
        item.set_links(vec![sales.clone(), purchase.clone(), sales]).unwrap();
        assert_eq!(item.links().len(), 3);
        assert!(item.links().contains(&purchase));

        item.subject_version = "2".into();
        item.set_links(vec![WorkItemLink::new("sales_order", "replacement").unwrap()]).unwrap();
        assert_eq!(item.links().len(), 2);
        assert!(!item.links().contains(&purchase));

        let before = item.clone();
        assert!(
            item.set_links(
                (0..1000).map(|i| WorkItemLink::new("sales_order", i.to_string()).unwrap()).collect()
            )
            .is_err()
        );
        assert_eq!(item, before);

        item.status = WorkItemStatus::Completed;
        let before = item.clone();
        assert!(item.set_links(Vec::new()).is_err());
        assert_eq!(item, before);
    }
    #[test]
    fn approval_links_reject_different_objects_and_submission_versions() {
        use bpm::ApprovalProcessInstanceId;
        use entity_core::BaseModel;
        use erp_core::common::time::Instant;

        use crate::entity::approval_integration::ApprovalSubjectSnapshotPayload;
        use crate::entity::document_registry::DocumentType;
        let snapshot = ApprovalSubjectSnapshot {
            base: BaseModel::fake(),
            approval_process_instance_id: ApprovalProcessInstanceId::new("instance"),
            document_type: DocumentType::SalesOrder,
            business_object_id: "sales".into(),
            subject_version: 2,
            payload: ApprovalSubjectSnapshotPayload {
                document_no: "SO-1".into(),
                responsible_org_id: "org".into(),
                submitted_by: "user".into(),
                submitted_at: Instant::from_unix_secs(1),
                counterparty: None,
                total_amount: None,
                total_quantity: None,
                line_count: 0,
            },
            links: vec![WorkItemLink::new("sales_order", "sales").unwrap()],
            display: None,
            material_files: Vec::new(),
            resubmission_reason: None,
        };
        let mut item = task();
        assert!(item.inherit_approval_links(&snapshot).is_err());
        item.business_object_type = "sales_order".into();
        item.business_object_id = "sales".into();
        assert!(item.inherit_approval_links(&snapshot).is_err());
        item.subject_version = "2".into();
        item.inherit_approval_links(&snapshot).unwrap();
        assert_eq!(item.links(), snapshot.links);
    }
}
