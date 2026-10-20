//! 审批抄送规则与个人收件事实；不产生审批责任。
use std::collections::{BTreeMap, BTreeSet};

use bpm::engine::{BpmEvent, BpmEventKind};
use bpm::model::ApprovalProcessInstance;
use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::entity::approval_integration::{ApprovalSubjectSnapshot, resolve_runtime_document_type};
use crate::entity::document_registry::DocumentType;
use crate::error::{Error, Result};

/// 抄送触发时机。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CopyTrigger {
    Started,
    NodeApproved,
    Approved,
    Rejected,
    Cancelled,
}

impl CopyTrigger {
    /// 从已接受的 BPM 事件提取抄送时机。
    /// # 参数
    /// `event` 为原事务内的事件。
    /// # 返回
    /// 恢复、受阻及节点进入不触发抄送。
    /// # 错误
    /// 无。
    pub fn from_event(event: &BpmEvent) -> Option<Self> {
        match event.kind {
            BpmEventKind::InstanceStarted => Some(Self::Started),
            BpmEventKind::NodeApproved => Some(Self::NodeApproved),
            BpmEventKind::InstanceApproved => Some(Self::Approved),
            BpmEventKind::NodeRejected => Some(Self::Rejected),
            BpmEventKind::InstanceCancelled => Some(Self::Cancelled),
            _ => None,
        }
    }
    /// 返回面向读者的抄送原因。
    /// # 参数
    /// 无。
    /// # 返回
    /// 固定中文原因。
    /// # 错误
    /// 无。
    pub fn label(self) -> &'static str {
        match self {
            Self::Started => "提交审批后抄送",
            Self::NodeApproved => "节点通过后抄送",
            Self::Approved => "审批通过后抄送",
            Self::Rejected => "审批驳回后抄送",
            Self::Cancelled => "审批撤回后抄送",
        }
    }
}

/// 定义内规则；节点顺序只在该不可变定义版本内解释。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApprovalCopyRule {
    pub trigger: CopyTrigger,
    pub node_order: Option<u32>,
    pub recipient_user_ids: Vec<String>,
}

impl ApprovalCopyRule {
    /// 规范化整组规则并校验人员、节点和规模。
    /// # 参数
    /// `rules` 为完整规则，`node_count` 为同一请求的节点数。
    /// # 返回
    /// 返回去重排序后的规则。
    /// # 错误
    /// 空人员、重复规则、非法节点及超限时拒绝。
    pub fn prepare(mut rules: Vec<Self>, node_count: usize) -> Result<Vec<Self>> {
        if rules.len() > 54 {
            return Err(Error::ValidationError("最多配置 54 条抄送规则".into()));
        }
        let mut seen = BTreeSet::new();
        for rule in &mut rules {
            let valid_node = match (rule.trigger, rule.node_order) {
                (CopyTrigger::NodeApproved, Some(order)) => {
                    order > 0 && usize::try_from(order).is_ok_and(|n| n <= node_count)
                },
                (CopyTrigger::NodeApproved, None) | (_, Some(_)) => false,
                (_, None) => true,
            };
            if !valid_node {
                return Err(Error::ValidationError("抄送节点不属于当前流程".into()));
            }
            if !seen.insert((rule.trigger.label(), rule.node_order)) {
                return Err(Error::ValidationError("同一抄送时机只能配置一次".into()));
            }
            if rule.recipient_user_ids.is_empty() || rule.recipient_user_ids.len() > 50 {
                return Err(Error::ValidationError("每条抄送规则须配置 1 至 50 人".into()));
            }
            for id in &mut rule.recipient_user_ids {
                *id = id.trim().to_owned();
                if id.is_empty() || id.len() > 128 {
                    return Err(Error::ValidationError("抄送人身份无效".into()));
                }
            }
            rule.recipient_user_ids.sort();
            rule.recipient_user_ids.dedup();
        }
        rules.sort_by_key(|r| (r.trigger.label(), r.node_order));
        Ok(rules)
    }

    /// 判断规则是否命中一次事件。
    /// # 参数
    /// `event` 和 `node_order` 来自实例绑定的定义。
    /// # 返回
    /// 时机和节点同时匹配时返回 true。
    /// # 错误
    /// 无。
    pub fn matches(&self, event: &BpmEvent, node_order: Option<u32>) -> bool {
        CopyTrigger::from_event(event) == Some(self.trigger)
            && (self.trigger != CopyTrigger::NodeApproved || self.node_order == node_order)
    }
}

/// 一个定义版本的一组抄送规则；ID 等于定义 ID。
#[derive(Debug, Clone, Serialize, Deserialize, Entity)]
pub struct ApprovalCopyConfig {
    #[serde(default)]
    pub recipient_names: BTreeMap<String, String>,
    #[serde(flatten)]
    pub base: BaseModel,
    pub rules: Vec<ApprovalCopyRule>,
}

/// 按事件和接收人保存的不可变收件事实；仅 read_at 可以更新。
#[derive(Debug, Clone, Serialize, Deserialize, Entity)]
pub struct ApprovalCopyReceipt {
    #[serde(flatten)]
    pub base: BaseModel,
    pub instance_id: String,
    pub snapshot_id: String,
    pub subject_version: u32,
    pub document_type: DocumentType,
    pub document_no: String,
    pub submitter_name: Option<String>,
    pub event_key: String,
    pub trigger: CopyTrigger,
    pub node_name: Option<String>,
    pub round_no: u32,
    pub recipient_user_id: String,
    pub recipient_name: String,
    pub delivered: bool,
    pub undelivered_reason: Option<String>,
    pub copied_at: i64,
    pub read_at: Option<i64>,
}

impl ApprovalCopyReceipt {
    /// 本人收件必须与同一实例和精确提交快照闭合。
    /// # 参数
    /// 认证账号、仓储读取的快照及实例。
    /// # 返回
    /// 全部身份和版本匹配且已送达时为 true。
    /// # 错误
    /// 无；非法运行类型失败关闭。
    pub fn matches_subject(
        &self,
        actor: &str,
        snapshot: &ApprovalSubjectSnapshot,
        instance: &ApprovalProcessInstance,
    ) -> bool {
        self.delivered
            && self.recipient_user_id == actor
            && self.instance_id == instance.base.id
            && self.snapshot_id == snapshot.base.id
            && self.instance_id == snapshot.approval_process_instance_id.as_ref()
            && self.document_type == snapshot.document_type
            && self.subject_version == snapshot.subject_version
            && snapshot
                .ensure_matches_runtime_subject(
                    self.document_type,
                    instance.subject.subject_id(),
                    instance.subject_version,
                )
                .is_ok()
            && resolve_runtime_document_type(instance.subject.subject_kind(), instance.process_kind)
                .is_ok_and(|kind| kind == self.document_type)
    }
}

#[cfg(test)]
mod tests {
    use bpm::ids::ApprovalProcessInstanceId;

    use super::*;
    #[test]
    fn rules_validate_nodes_and_normalize_recipients() {
        let rule = ApprovalCopyRule {
            trigger: CopyTrigger::NodeApproved,
            node_order: Some(1),
            recipient_user_ids: vec![" b ".into(), "a".into(), "b".into()],
        };
        assert_eq!(
            ApprovalCopyRule::prepare(vec![rule.clone()], 1).unwrap()[0].recipient_user_ids,
            vec!["a", "b"]
        );
        assert!(ApprovalCopyRule::prepare(vec![rule.clone()], 0).is_err());
        assert!(ApprovalCopyRule::prepare(vec![rule.clone(), rule], 1).is_err());
    }
    #[test]
    fn recovery_does_not_repeat_copy_and_node_must_match() {
        let mut event =
            BpmEvent::new(BpmEventKind::AssigneeRecovered, ApprovalProcessInstanceId::new("i"), 1);
        assert_eq!(CopyTrigger::from_event(&event), None);
        event.kind = BpmEventKind::NodeApproved;
        let rule = ApprovalCopyRule {
            trigger: CopyTrigger::NodeApproved,
            node_order: Some(2),
            recipient_user_ids: vec!["u".into()],
        };
        assert!(!rule.matches(&event, Some(1)));
        assert!(rule.matches(&event, Some(2)));
    }
    fn receipt() -> ApprovalCopyReceipt {
        ApprovalCopyReceipt {
            base: BaseModel::fake(),
            instance_id: "instance".into(),
            snapshot_id: "snapshot".into(),
            subject_version: 1,
            document_type: DocumentType::StockAdjustment,
            document_no: "ADJ-1".into(),
            submitter_name: None,
            event_key: "started".into(),
            trigger: CopyTrigger::Started,
            node_name: None,
            round_no: 1,
            recipient_user_id: "reader".into(),
            recipient_name: "读者".into(),
            delivered: true,
            undelivered_reason: None,
            copied_at: 1,
            read_at: None,
        }
    }
    #[test]
    fn copy_read_rejects_other_users_instances_versions_and_undelivered() {
        use bpm::ids::ApprovalProcessDefinitionId;
        use bpm::model::{NewProcessInstance, ParticipantId, ProcessKind, SubjectRef, Timestamp};
        use erp_core::common::time::Instant;

        use crate::entity::approval_integration::ApprovalSubjectSnapshotPayload;
        let instance = ApprovalProcessInstance::start_running(NewProcessInstance {
            id: ApprovalProcessInstanceId::new("instance"),
            process_definition_id: ApprovalProcessDefinitionId::new("def"),
            definition_version: 1,
            process_kind: ProcessKind::StockAdjustment,
            subject: SubjectRef::new("stock_adjustment", "adj").unwrap(),
            subject_version: 1,
            started_by: ParticipantId::new("submitter").unwrap(),
            at: Timestamp::from_unix_secs(1).unwrap(),
        })
        .unwrap();
        let mut snapshot = ApprovalSubjectSnapshot {
            base: BaseModel::fake(),
            approval_process_instance_id: ApprovalProcessInstanceId::new("instance"),
            document_type: DocumentType::StockAdjustment,
            business_object_id: "adj".into(),
            subject_version: 1,
            payload: ApprovalSubjectSnapshotPayload {
                document_no: "ADJ-1".into(),
                responsible_org_id: "org".into(),
                submitted_by: "submitter".into(),
                submitted_at: Instant::now(),
                counterparty: None,
                total_amount: None,
                total_quantity: None,
                line_count: 0,
            },
            display: None,
            material_files: vec![],
        };
        snapshot.base.id = "snapshot".into();
        let mut record = receipt();
        assert!(record.matches_subject("reader", &snapshot, &instance));
        assert!(!record.matches_subject("other", &snapshot, &instance));
        record.delivered = false;
        assert!(!record.matches_subject("reader", &snapshot, &instance));
        record.delivered = true;
        snapshot.subject_version = 2;
        assert!(!record.matches_subject("reader", &snapshot, &instance));
        snapshot.subject_version = 1;
        snapshot.approval_process_instance_id = ApprovalProcessInstanceId::new("new-instance");
        assert!(!record.matches_subject("reader", &snapshot, &instance));
    }
    #[test]
    fn empty_oversized_and_invalid_trigger_rules_are_rejected() {
        let rule =
            ApprovalCopyRule { trigger: CopyTrigger::Approved, node_order: None, recipient_user_ids: vec![] };
        assert!(ApprovalCopyRule::prepare(vec![rule.clone()], 1).is_err());
        let mut invalid = rule.clone();
        invalid.recipient_user_ids = vec!["u".into(); 51];
        assert!(ApprovalCopyRule::prepare(vec![invalid], 1).is_err());
        let mut invalid = rule;
        invalid.recipient_user_ids = vec!["u".into()];
        invalid.node_order = Some(1);
        assert!(ApprovalCopyRule::prepare(vec![invalid], 1).is_err());
    }
}
