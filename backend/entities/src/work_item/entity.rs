//! `work_item`：审批与独立人工任务的当前责任事实。

mod approval;
mod permissions;
mod status;
mod types;
mod validation;

pub use approval::{ApprovalDecisionTaskError, ApprovalRuntimeTaskEnding, DocumentApprovalWorkItemData};
pub use permissions::AvailableWorkItemAccount;
pub use status::WorkItemCloseData;
pub use types::{
    AssignmentSource, WorkItemAssignmentSeparationPolicy, WorkItemBriefObjectKind, WorkItemBriefRelation,
    WorkItemPriority, WorkItemStatus, WorkItemType,
};
pub use validation::{WorkItemData, WorkItemSubjectVersions};

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::common::time::Instant;
use crate::errors::{Error, Result};
#[cfg(test)]
use crate::ids::WorkItemId;
#[cfg(test)]
use crate::{AccountCore, AccountCoreData, AccountKind, AccountStatus, LoginAccount, Secret};

use super::FulfillmentResponsibilityKey;

use bpm::ApprovalNodeExecutionId;

/// 当前人工责任事实。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct WorkItem {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 固定任务类型。
    pub work_item_type: WorkItemType,
    /// 类型化审批节点执行；审批任务必填。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approval_node_execution_id: Option<ApprovalNodeExecutionId>,
    /// 业务对象类型。
    pub business_object_type: String,
    /// 业务对象 ID。
    pub business_object_id: String,
    /// 服务端冻结的可选责任维度；普通任务为空，存在时参与开放任务唯一性。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    responsibility_key: Option<String>,
    /// 服务端冻结的稳定业务行范围；普通任务为空。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    responsibility_scope_ids: Vec<String>,
    /// 被处理的不可变提交或业务版本。
    pub subject_version: String,
    /// 生命周期状态。
    pub status: WorkItemStatus,
    /// 责任角色。
    pub owner_role: String,
    /// 责任组织。
    pub owner_organization_id: String,
    /// 当前个人责任人；开放任务必填。
    pub owner_user_id: Option<String>,
    /// 曾形成个人责任的用户 ID；只追加、去重，退回与终态动作均保留。
    pub responsibility_actor_ids: Vec<String>,
    /// 当前或最近一次责任来源。
    pub assignment_source: AssignmentSource,
    /// 首次形成个人责任的时间。
    pub assigned_at: Option<Instant>,
    /// 首次正式处理时间。
    pub started_at: Option<Instant>,
    /// 当前个人责任生效时间。
    pub current_assignment_at: Option<Instant>,
    /// 最近一次非只读活动时间。
    pub last_activity_at: Option<Instant>,
    /// 优先级。
    pub priority: WorkItemPriority,
    /// 时限。
    pub due_at: Option<Instant>,
    /// 产生原因代码。
    pub reason_code: Option<String>,
    /// 业务影响摘要。
    pub impact_summary: Option<String>,
    /// 正式完成时间。
    pub completed_at: Option<Instant>,
    /// 正式完成人。
    pub completed_by: Option<String>,
    /// 受控关闭时间。
    pub closed_at: Option<Instant>,
    /// 受控关闭操作人。
    pub closed_by: Option<String>,
    /// 受控关闭原因。
    pub close_reason: Option<String>,
}

impl WorkItem {
    /// 返回创建时冻结的责任维度。
    ///
    /// # 返回
    /// 普通任务返回 `None`；采用多责任维度开放唯一性的任务返回固定键。
    pub fn responsibility_key(&self) -> Option<&str> {
        self.responsibility_key.as_deref()
    }

    /// 解析并校验履约任务冻结的责任键与对象、角色、原因合同。
    ///
    /// # 返回
    /// 非履约任务返回 `None`；已注册履约任务返回强类型责任键。
    ///
    /// # 错误
    /// 履约任务缺少责任键，或对象、角色、原因与责任键类型不一致时返回错误。
    pub fn fulfillment_responsibility_key(&self) -> Result<Option<FulfillmentResponsibilityKey>> {
        if !self.work_item_type.is_fulfillment_operation() {
            return Ok(None);
        }
        let key = self
            .responsibility_key()
            .ok_or_else(|| Error::from("履约任务缺少责任键"))
            .and_then(FulfillmentResponsibilityKey::parse)?;
        let matches = matches!(
            (
                self.business_object_type.as_str(),
                self.owner_role.as_str(),
                self.reason_code.as_deref(),
                &key,
            ),
            (
                "delivery",
                "purchase_order_owner",
                Some("SUPPLIER_DIRECT_DELIVERY_READY"),
                FulfillmentResponsibilityKey::PurchaseOrder(_),
            ) | (
                "electronic_delivery",
                "purchase_order_owner",
                Some("ELECTRONIC_DELIVERY_READY"),
                FulfillmentResponsibilityKey::PurchaseOrder(_),
            ) | (
                "service_fulfillment",
                "purchase_order_owner",
                Some("SERVICE_FULFILLMENT_READY"),
                FulfillmentResponsibilityKey::PurchaseOrder(_),
            ) | (
                "purchase_receipt",
                "warehouse_inbound_handler",
                Some("PURCHASE_RECEIPT_READY"),
                FulfillmentResponsibilityKey::WarehouseReceipt(_),
            ) | (
                "delivery",
                "warehouse_outbound_handler",
                Some("WAREHOUSE_DELIVERY_READY"),
                FulfillmentResponsibilityKey::WarehouseShip(_),
            )
        );
        if !matches {
            return Err(Error::from("履约任务对象、责任角色、原因或责任键不一致"));
        }
        Ok(Some(key))
    }

    /// 返回创建时冻结的稳定业务行范围。
    ///
    /// # 返回
    /// 返回按稳定 ID 排序并去重的只读切片；普通任务返回空切片。
    pub fn responsibility_scope_ids(&self) -> &[String] {
        &self.responsibility_scope_ids
    }

    /// 判断任务是否绑定指定业务对象身份。
    ///
    /// # 参数
    /// * `business_object_type` - 期望业务对象类型
    /// * `business_object_id` - 期望业务对象 ID
    ///
    /// # 返回
    /// 类型与稳定 ID 均匹配时返回 `true`。
    pub fn matches_business_object(&self, business_object_type: &str, business_object_id: &str) -> bool {
        self.business_object_type == business_object_type && self.business_object_id == business_object_id
    }

    /// 判断任务是否冻结指定业务对象版本。
    ///
    /// # 参数
    /// * `subject_version` - 权威对象版本
    ///
    /// # 返回
    /// 与任务冻结版本一致时返回 `true`。
    pub fn matches_subject_version(&self, subject_version: &str) -> bool {
        self.subject_version == subject_version
    }

    /// 判断任务是否属于 W29 可受控关闭关系。
    ///
    /// # 返回
    /// 非审批的集成异常或对账差异任务返回 `true`。
    pub fn is_w29_closable(&self) -> bool {
        self.work_item_type.is_w29_closable(
            &self.business_object_type,
            self.approval_node_execution_id.is_some(),
        )
    }

    /// 判断本任务可否作为另一 W29 任务的正式替代任务。
    ///
    /// # 参数
    /// * `current` - 待关闭的当前任务
    ///
    /// # 返回
    /// 本任务不同于当前任务、仍开放、同任务类型且同对象类别时返回 `true`。
    pub fn is_w29_replacement_for(&self, current: &Self) -> bool {
        self.base.id != current.base.id
            && self.status == WorkItemStatus::Open
            && self.is_w29_closable()
            && self.work_item_type == current.work_item_type
            && self.business_object_type == current.business_object_type
    }
}

#[cfg(test)]
/// 构造独立任务的最小测试数据。
///
/// # 返回
/// 返回带可规范化空白和固定责任人的输入。
fn direct_data() -> WorkItemData {
    WorkItemData {
        work_item_type: WorkItemType::ImportBusinessConfirmation,
        business_object_type: " LEGACY_IMPORT_BATCH ".to_string(),
        business_object_id: " batch-1 ".to_string(),
        subject_version: " v3 ".to_string(),
        owner_role: " sales ".to_string(),
        owner_organization_id: " org-1 ".to_string(),
        owner_user_id: " alice ".to_string(),
        assignment_source: AssignmentSource::SystemRule,
        priority: WorkItemPriority::Normal,
        due_at: Some(Instant::from_unix_secs(1_700_086_400)),
        reason_code: Some("IMPORT_READY".to_string()),
        impact_summary: Some(" 待确认导入范围 ".to_string()),
    }
}

#[cfg(test)]
/// 构造绑定固定节点执行的开放审批任务。
///
/// # 返回
/// 返回责任人为 `alice`、执行为 `exec-1` 的任务。
fn approval_item(id: &str) -> WorkItem {
    WorkItem::new_document_approval(
        WorkItemId::new(id),
        DocumentApprovalWorkItemData {
            approval_node_execution_id: bpm::ApprovalNodeExecutionId::new("exec-1"),
            business_object_type: "stock_adjustment".into(),
            business_object_id: "adj-1".into(),
            subject_version: "1".into(),
            owner_role: "stock_adjustment_approver".into(),
            owner_organization_id: "org-1".into(),
            owner_user_id: " alice ".into(),
            priority: WorkItemPriority::Normal,
            due_at: None,
        },
        Instant::from_unix_secs(100),
    )
    .unwrap()
}

#[cfg(test)]
fn account(status: AccountStatus) -> AccountCore {
    AccountCore::new(
        "account-1".to_string(),
        AccountCoreData {
            secret: Secret::new(LoginAccount::new("worker").unwrap(), "password123").unwrap(),
            name: "处理人".to_string(),
            kind: AccountKind::Admin,
            status,
            email: None,
            phone: None,
            avatar: None,
        },
    )
    .unwrap()
}

#[cfg(test)]
mod tests {
    use super::{direct_data, AssignmentSource, WorkItem, WorkItemType};
    use crate::common::time::Instant;
    use crate::ids::WorkItemId;

    #[test]
    fn codes_and_bson_shape_are_stable() {
        assert_eq!(AssignmentSource::SystemRule.as_str(), "SYSTEM_RULE");
        assert_eq!(WorkItemType::DocumentApproval.as_str(), "DOCUMENT_APPROVAL");
        let item = WorkItem::new_at(
            WorkItemId::new("wi-1"),
            direct_data(),
            Instant::from_unix_secs(100),
        )
        .unwrap();
        let document = bson::serialize_to_document(&item).unwrap();
        assert_eq!(document.get_str("status").unwrap(), "OPEN");
        let roundtrip: WorkItem = bson::deserialize_from_document(document).unwrap();
        assert_eq!(roundtrip, item);
    }
}
