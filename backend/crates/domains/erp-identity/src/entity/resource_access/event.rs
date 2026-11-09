//! 授权业务事件及独立处理状态；事件类型决定所需证据与来源身份。
use entity_core::BaseModel;
use entity_macros::Entity;
use erp_core::validation::normalize_required_text_ref;
use serde::{Deserialize, Serialize};

use super::SourceKey;
use crate::{Error, Result};

/// 业务触发与必需证据。开票仅在最终审批通过并创建执行任务后产生事件。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum GrantEventPayload {
    InvoiceApproved {
        invoice_request_id: String,
        approval_instance_id: String,
        work_item_id: String,
    },
    SalesOrderApprovalAssigned {
        sales_order_id: String,
        approval_instance_id: String,
        node_id: String,
        assignment_id: String,
    },
    PurchaseOrderApprovalAssigned {
        purchase_order_id: String,
        approval_instance_id: String,
        node_id: String,
        assignment_id: String,
    },
}
impl GrantEventPayload {
    /// 从类型化业务证据生成固定规则与来源，禁止调用方任意搭配字符串。
    /// # 参数
    /// recipient 为正式执行任务或审批节点的实际分配账号。
    /// # 返回
    /// 与事件变体一致的稳定来源身份；节点及分配证据仍保存在事件中。
    /// # 错误
    /// 证据或接收账号为空、超长、含首尾空白时拒绝。
    pub fn source(&self, recipient: &str) -> Result<SourceKey> {
        let (rule, business_type, business_id, ids) = match self {
            Self::InvoiceApproved { invoice_request_id, approval_instance_id, work_item_id } => (
                "invoice_approved",
                "sales_invoice_request",
                invoice_request_id,
                vec![invoice_request_id, approval_instance_id, work_item_id],
            ),
            Self::SalesOrderApprovalAssigned {
                sales_order_id,
                approval_instance_id,
                node_id,
                assignment_id,
            } => (
                "sales_order_approval_assigned",
                "approval_instance",
                approval_instance_id,
                vec![sales_order_id, approval_instance_id, node_id, assignment_id],
            ),
            Self::PurchaseOrderApprovalAssigned {
                purchase_order_id,
                approval_instance_id,
                node_id,
                assignment_id,
            } => (
                "purchase_order_approval_assigned",
                "approval_instance",
                approval_instance_id,
                vec![purchase_order_id, approval_instance_id, node_id, assignment_id],
            ),
        };
        for id in ids {
            if normalize_required_text_ref(id, "业务事件证据不能为空", 128, "业务事件证据过长")? != *id
            {
                return Err(Error::ValidationError("业务事件证据不能带首尾空白".into()));
            }
        }
        let source = SourceKey {
            rule: rule.into(),
            business_type: business_type.into(),
            business_id: business_id.clone(),
            recipient: recipient.into(),
        };
        source.id()?;
        Ok(source)
    }

    /// 提取稳定事件身份，不将可变关联目标混入幂等键。
    fn identity(&self) -> (&'static str, &str, &str, &str) {
        match self {
            Self::InvoiceApproved { invoice_request_id, approval_instance_id, .. } => {
                ("invoice_approved", invoice_request_id, approval_instance_id, "")
            },
            Self::SalesOrderApprovalAssigned { approval_instance_id, node_id, assignment_id, .. } => {
                ("sales_order_approval_assigned", approval_instance_id, node_id, assignment_id)
            },
            Self::PurchaseOrderApprovalAssigned { approval_instance_id, node_id, assignment_id, .. } => {
                ("purchase_order_approval_assigned", approval_instance_id, node_id, assignment_id)
            },
        }
    }
}

/// 来源业务已正式提交的事件事实；不接受客户端自选接收人。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GrantEventEvidence {
    pub payload: GrantEventPayload,
    pub recipient: String,
    /// 开票使用申请版本；销售、采购审批使用审批实例版本，禁止混用节点局部版本。
    pub fact_version: u64,
    pub occurred_at: u64,
}
/// 处理状态独立于业务事件类型；领取与重试由后续 worker 实现。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GrantEventStatus {
    Pending,
    Processing,
    Applied,
    Failed,
}
#[derive(Debug, Clone, Serialize, Deserialize, Entity)]
pub struct GrantEvent {
    #[serde(flatten)]
    pub base: BaseModel,
    pub source: SourceKey,
    pub evidence: GrantEventEvidence,
    pub status: GrantEventStatus,
    pub attempts: u32,
    pub available_at: u64,
    pub lease_owner: Option<String>,
    pub lease_until: Option<u64>,
    pub failure: Option<String>,
}
impl GrantEvent {
    /// 由业务事件类型、正式业务身份、接收人及事实版本构造幂等事件。
    /// # 参数
    /// evidence 来自开票审批通过或审批节点正式分配事务。
    /// # 返回
    /// 含派生来源身份及原发生时间的 Pending 事件。
    /// # 错误
    /// 业务证据非法、零版本或零发生时间时拒绝。
    pub fn new(evidence: GrantEventEvidence) -> Result<Self> {
        let source = evidence.payload.source(&evidence.recipient)?;
        if evidence.fact_version == 0 || evidence.occurred_at == 0 {
            return Err(Error::ValidationError("业务事件事实版本和发生时间必须为正数".into()));
        }
        let id =
            serde_json::to_string(&(evidence.payload.identity(), &evidence.recipient, evidence.fact_version))
                .map_err(|e| Error::Internal(e.to_string()))?;
        Ok(Self {
            base: BaseModel::new(id),
            source,
            available_at: evidence.occurred_at,
            evidence,
            status: GrantEventStatus::Pending,
            attempts: 0,
            lease_owner: None,
            lease_until: None,
            failure: None,
        })
    }

    /// 验证相同幂等键对应的完整证据，拒绝悄悄替换任务、单据或发生时间。
    /// # 参数
    /// incoming 为本次构造并校验的事件。
    /// # 返回
    /// 同键同来源同证据时成功；原处理状态不影响幂等重放。
    /// # 错误
    /// 键、来源或载荷不一致返回冲突。
    pub fn ensure_replay(&self, incoming: &Self) -> Result<()> {
        if self.base.id != incoming.base.id
            || self.source != incoming.source
            || self.evidence != incoming.evidence
        {
            return Err(Error::ConflictError("同事件键的业务证据不一致".into()));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::*;

    fn payloads() -> Vec<GrantEventPayload> {
        vec![
            GrantEventPayload::InvoiceApproved {
                invoice_request_id: "request-1".into(),
                approval_instance_id: "approval-1".into(),
                work_item_id: "task-1".into(),
            },
            GrantEventPayload::SalesOrderApprovalAssigned {
                sales_order_id: "order-1".into(),
                approval_instance_id: "approval-1".into(),
                node_id: "node-1".into(),
                assignment_id: "assignment-1".into(),
            },
            GrantEventPayload::PurchaseOrderApprovalAssigned {
                purchase_order_id: "order-1".into(),
                approval_instance_id: "approval-1".into(),
                node_id: "node-1".into(),
                assignment_id: "assignment-1".into(),
            },
        ]
    }
    fn evidence(payload: GrantEventPayload) -> GrantEventEvidence {
        GrantEventEvidence { payload, recipient: "operator-1".into(), fact_version: 1, occurred_at: 100 }
    }
    #[test]
    fn typed_events_round_trip_json_and_derive_sources() {
        let cases = payloads().into_iter().zip([
            ("invoice_approved", "sales_invoice_request", "request-1"),
            ("sales_order_approval_assigned", "approval_instance", "approval-1"),
            ("purchase_order_approval_assigned", "approval_instance", "approval-1"),
        ]);
        for (payload, (kind, business_type, business_id)) in cases {
            let value = serde_json::to_value(&payload).unwrap();
            assert_eq!(value["type"], kind);
            assert_eq!(serde_json::from_value::<GrantEventPayload>(value).unwrap(), payload);
            let event = GrantEvent::new(evidence(payload)).unwrap();
            assert_eq!(event.source.rule, kind);
            assert_eq!(event.source.business_type, business_type);
            assert_eq!(event.source.business_id, business_id);
            assert_eq!(event.source.recipient, "operator-1");
            assert_eq!(event.status, GrantEventStatus::Pending);
            assert_eq!(event.available_at, 100);
        }
    }
    #[test]
    fn event_types_recipients_and_versions_have_independent_keys() {
        let events: Vec<_> = payloads().into_iter().map(|p| GrantEvent::new(evidence(p)).unwrap()).collect();
        for i in 0..events.len() {
            for j in i + 1..events.len() {
                assert_ne!(events[i].base.id, events[j].base.id);
            }
        }
        let original = &events[0];
        let mut changed = original.evidence.clone();
        changed.recipient = "operator-2".into();
        assert_ne!(original.base.id, GrantEvent::new(changed).unwrap().base.id);
        let mut changed = original.evidence.clone();
        changed.fact_version += 1;
        assert_ne!(original.base.id, GrantEvent::new(changed).unwrap().base.id);
    }
    #[test]
    fn replay_preserves_processing_state_and_rejects_changed_business_evidence() {
        for payload in payloads() {
            let mut original = GrantEvent::new(evidence(payload)).unwrap();
            let retry = GrantEvent::new(original.evidence.clone()).unwrap();
            original.status = GrantEventStatus::Applied;
            original.ensure_replay(&retry).unwrap();
            let mut changed = original.evidence.clone();
            match &mut changed.payload {
                GrantEventPayload::InvoiceApproved { work_item_id, .. } => {
                    *work_item_id = "other-task".into()
                },
                GrantEventPayload::SalesOrderApprovalAssigned { sales_order_id, .. } => {
                    *sales_order_id = "other-order".into()
                },
                GrantEventPayload::PurchaseOrderApprovalAssigned { purchase_order_id, .. } => {
                    *purchase_order_id = "other-order".into()
                },
            }
            let changed = GrantEvent::new(changed).unwrap();
            assert_eq!(original.base.id, changed.base.id);
            assert!(matches!(original.ensure_replay(&changed), Err(Error::ConflictError(_))));
            let mut changed = original.evidence.clone();
            changed.occurred_at += 1;
            assert!(original.ensure_replay(&GrantEvent::new(changed).unwrap()).is_err());
        }
    }
    #[test]
    fn invalid_or_missing_evidence_fails_closed_for_every_variant() {
        for payload in payloads() {
            let value = serde_json::to_value(&payload).unwrap();
            for key in value.as_object().unwrap().keys().filter(|k| *k != "type") {
                let mut missing = value.clone();
                missing.as_object_mut().unwrap().remove(key);
                assert!(serde_json::from_value::<GrantEventPayload>(missing).is_err());
                for invalid in ["".to_string(), " value ".into(), "x".repeat(129)] {
                    let mut bad = value.clone();
                    bad[key] = Value::String(invalid);
                    let parsed = serde_json::from_value::<GrantEventPayload>(bad).unwrap();
                    assert!(GrantEvent::new(evidence(parsed)).is_err());
                }
            }
            let mut bad = evidence(payload.clone());
            bad.fact_version = 0;
            assert!(GrantEvent::new(bad).is_err());
            let mut bad = evidence(payload.clone());
            bad.occurred_at = 0;
            assert!(GrantEvent::new(bad).is_err());
            let mut bad = evidence(payload);
            bad.recipient.clear();
            assert!(GrantEvent::new(bad).is_err());
        }
        for kind in ["invoice_task_assigned", "unknown"] {
            assert!(serde_json::from_value::<GrantEventPayload>(json!({ "type": kind })).is_err());
        }
        let mut bad = serde_json::to_value(&payloads()[0]).unwrap();
        bad["assignment_id"] = json!("unexpected");
        assert!(serde_json::from_value::<GrantEventPayload>(bad).is_err());
    }
}
