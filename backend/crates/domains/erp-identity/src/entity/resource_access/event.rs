//! 授权业务事件及独立处理状态；事件类型决定所需证据与来源身份。
use entity_core::BaseModel;
use entity_macros::Entity;
use erp_core::validation::normalize_required_text_ref;
use id_generator::next_id;
use serde::{Deserialize, Serialize};

use super::SourceKey;
use crate::{Error, Result};

/// 业务触发与必需证据。开票仅在最终审批通过并创建执行任务后产生事件。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum GrantEventPayload {
    InvoiceTaskReassigned {
        /// 本次转派的开票执行任务 ID，用于回查任务及其关联申请。
        work_item_id: String,
        /// 正式转派命令的回执 ID，用于去重并复验实际接收人。
        command_receipt_id: String,
    },
    InvoiceApproved {
        /// 最终审批通过的开票申请 ID，也是授权来源的业务主键。
        invoice_request_id: String,
        /// 最终批准该申请的审批实例 ID。
        approval_instance_id: String,
        /// 最终审批通过时创建的开票执行任务 ID，用于复验正式分配。
        work_item_id: String,
    },
    SalesOrderApprovalAssigned {
        /// 本次审批实际关联的销售单 ID，也是待复验的读取目标。
        sales_order_id: String,
        /// 产生本次销售单审批分配的实例 ID，也是来源业务主键。
        approval_instance_id: String,
        /// 本次分配对应的审批节点键，用于和节点执行记录交叉核验。
        node_id: String,
        /// 产生销售单正式分配的节点执行记录 ID。
        assignment_id: String,
    },
    PurchaseOrderApprovalAssigned {
        /// 本次审批实际关联的采购单 ID，也是待复验的读取目标。
        purchase_order_id: String,
        /// 产生本次采购单审批分配的实例 ID，也是来源业务主键。
        approval_instance_id: String,
        /// 本次分配对应的审批节点键，用于确认实际分配节点。
        node_id: String,
        /// 产生采购单正式分配的节点执行记录 ID。
        assignment_id: String,
    },
}
impl GrantEventPayload {
    /// 根据事件变体固定授权规则、业务类型和业务 ID，节点及分配等细节保留在事件证据中。
    /// 接收账号须来自正式任务或审批节点的实际分配；证据 ID 或账号为空、超长或带首尾空白时拒绝。
    pub fn source(&self, recipient: &str) -> Result<SourceKey> {
        let (rule, business_type, business_id, ids) = match self {
            Self::InvoiceTaskReassigned { work_item_id, command_receipt_id } => {
                ("invoice_task_reassigned", "work_item", work_item_id, vec![work_item_id, command_receipt_id])
            },
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

    // 提取事件类型和该业务动作的稳定证据键；未参与键的关联记录仍在重放时比对。
    fn identity(&self) -> (&'static str, &str, &str, &str) {
        match self {
            Self::InvoiceTaskReassigned { command_receipt_id, .. } => {
                ("invoice_task_reassigned", command_receipt_id, "", "")
            },
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

/// 由业务事务提供的正式触发事实；接收账号必须来自真实分配。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GrantEventEvidence {
    /// 标明触发类型并携带回查正式业务分配所需的证据。
    pub payload: GrantEventPayload,
    /// 任务或审批节点实际分配的后台账号 ID，禁止由客户端自选。
    pub recipient: String,
    /// 触发时的业务事实版本：开票通过用申请版本，转派用任务版本，订单审批分配用审批实例版本。
    pub fact_version: u64,
    /// 原业务动作发生的秒级时间，也是事件初次可领取时间；重放时须保持一致。
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
/// 在业务事务中追加、由后台异步处理的记录读取授权事件；追加时授权尚未发布。
#[derive(Debug, Clone, Serialize, Deserialize, Entity)]
pub struct GrantEvent {
    /// 事件队列记录的存储 ID 和版本，独立于业务幂等键。
    #[serde(flatten)]
    pub base: BaseModel,
    /// 由事件类型、稳定业务证据键、接收账号及事实版本组成的唯一键，用于识别重放。
    pub dedup_key: String,
    /// 从业务证据派生的稳定来源身份，授权发布时用它定位来源记录。
    pub source: SourceKey,
    /// 供重放比较和后台处理器回查的完整业务证据。
    pub evidence: GrantEventEvidence,
    /// 后台队列的处理状态，独立于触发事件的业务类型。
    pub status: GrantEventStatus,
    /// 累计领取次数，包含租约到期后的重新领取，用于计算失败退避。
    pub attempts: u32,
    /// 待处理或失败事件最早可领取的秒级时间，失败后按退避推进。
    pub available_at: u64,
    /// 当前领取的唯一令牌，完成或失败回写前用于识别持有者。
    pub lease_owner: Option<String>,
    /// 当前领取的秒级到期时间，到期后队列可重新分配事件。
    pub lease_until: Option<u64>,
    /// 最近一次失败回写的原因，成功完成后清除。
    pub failure: Option<String>,
}
impl GrantEvent {
    /// 将正式业务证据构造成待领取事件，派生来源与幂等键，首次可领取时间取业务发生时间。
    /// 正式证据非法、事实版本为零或发生时间为零时拒绝。
    pub fn new(evidence: GrantEventEvidence) -> Result<Self> {
        let source = evidence.payload.source(&evidence.recipient)?;
        if evidence.fact_version == 0 || evidence.occurred_at == 0 {
            return Err(Error::ValidationError("业务事件事实版本和发生时间必须为正数".into()));
        }

        let dedup_key = serde_json::to_string(&(
            evidence.payload.identity(),
            &evidence.recipient,
            evidence.fact_version,
        ))?;
        Ok(Self {
            base: BaseModel::new(next_id()),
            dedup_key,
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

    /// 追加重放时比较幂等键、来源和完整证据（含业务发生时间）；任一变化都返回冲突。
    /// 队列记录 ID 和处理状态不参与此次比较。
    pub fn ensure_replay(&self, incoming: &Self) -> Result<()> {
        if self.dedup_key != incoming.dedup_key
            || self.source != incoming.source
            || self.evidence != incoming.evidence
        {
            return Err(Error::ConflictError("同事件键的业务证据不一致".into()));
        }
        Ok(())
    }
}

impl GrantEvent {
    /// 发布前核对原领取快照的记录 ID 和幂等证据；相同事件已完成时允许重放。
    /// 未完成事件须仍在处理中，且持有与原快照相同的非空领取令牌；否则返回冲突。
    pub fn ensure_claim(&self, claim: &Self) -> Result<()> {
        if self.base.id != claim.base.id {
            return Err(Error::ConflictError("授权事件记录不一致".into()));
        }
        self.ensure_replay(claim)?;

        if self.status == GrantEventStatus::Applied {
            return Ok(());
        }
        if self.status != GrantEventStatus::Processing
            || self.lease_owner.as_deref().is_none_or(str::is_empty)
            || self.lease_owner != claim.lease_owner
        {
            return Err(Error::ConflictError("授权事件领取已失效".into()));
        }
        Ok(())
    }
    /// 根据已领取次数计算失败后的指数退避秒数，上限为 900 秒。
    pub fn retry_delay(&self) -> u64 {
        2_u64.pow(self.attempts.min(10)).min(900)
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::*;

    // 为四种授权触发类型构造合法的业务证据样本，供序列化和校验测试复用。
    fn payloads() -> Vec<GrantEventPayload> {
        vec![
            GrantEventPayload::InvoiceTaskReassigned {
                work_item_id: "task-1".into(),
                command_receipt_id: "command-1".into(),
            },
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
    // 为测试载荷补齐固定接收账号、正版本和秒级事件时间。
    fn evidence(payload: GrantEventPayload) -> GrantEventEvidence {
        GrantEventEvidence { payload, recipient: "operator-1".into(), fact_version: 1, occurred_at: 100 }
    }
    // 验证四类载荷可按类型标记往返 JSON，并派生预期的来源和初始队列状态。
    #[test]
    fn typed_events_round_trip_json_and_derive_sources() {
        let cases = payloads().into_iter().zip([
            ("invoice_task_reassigned", "work_item", "task-1"),
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
    // 验证事件类型、接收账号或事实版本的变化会区分幂等键。
    #[test]
    fn event_types_recipients_and_versions_have_independent_keys() {
        let events: Vec<_> = payloads().into_iter().map(|p| GrantEvent::new(evidence(p)).unwrap()).collect();
        for i in 0..events.len() {
            for j in i + 1..events.len() {
                assert_ne!(events[i].dedup_key, events[j].dedup_key);
            }
        }

        let original = &events[0];
        let mut changed = original.evidence.clone();
        changed.recipient = "operator-2".into();
        assert_ne!(original.dedup_key, GrantEvent::new(changed).unwrap().dedup_key);

        let mut changed = original.evidence.clone();
        changed.fact_version += 1;
        assert_ne!(original.dedup_key, GrantEvent::new(changed).unwrap().dedup_key);
    }
    // 验证同证据能重放已处理事件，同键下更换关联证据或发生时间会触发冲突。
    #[test]
    fn replay_preserves_processing_state_and_rejects_changed_business_evidence() {
        for payload in payloads() {
            let mut original = GrantEvent::new(evidence(payload)).unwrap();
            let retry = GrantEvent::new(original.evidence.clone()).unwrap();
            assert_ne!(original.base.id, retry.base.id);
            assert_eq!(original.dedup_key, retry.dedup_key);
            original.status = GrantEventStatus::Applied;
            original.ensure_replay(&retry).unwrap();

            let mut changed = original.evidence.clone();
            match &mut changed.payload {
                GrantEventPayload::InvoiceTaskReassigned { work_item_id, .. } => {
                    *work_item_id = "other".into()
                },
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
            assert_eq!(original.dedup_key, changed.dedup_key);
            assert!(matches!(original.ensure_replay(&changed), Err(Error::ConflictError(_))));

            let mut changed = original.evidence.clone();
            changed.occurred_at += 1;
            assert!(original.ensure_replay(&GrantEvent::new(changed).unwrap()).is_err());
        }
    }
    // 逐类验证缺失或非法证据字段、零版本、零时间、空接收账号及未知类型或字段均会失败。
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
    // 验证回填事件可按幂等键匹配新事件，同时保留迁移前的记录 ID、处理状态和领取次数。
    #[test]
    fn migrated_event_replays_without_replacing_legacy_id_or_state() {
        let incoming = GrantEvent::new(evidence(payloads().remove(0))).unwrap();
        let legacy_id = r#"[["invoice_task_reassigned","command-1","",""],"operator-1",1]"#;
        assert_eq!(incoming.dedup_key, legacy_id);

        let mut stored = incoming.clone();
        stored.base.id = legacy_id.into();
        stored.status = GrantEventStatus::Applied;
        stored.attempts = 3;
        stored.ensure_replay(&incoming).unwrap();
        assert_eq!(stored.base.id, legacy_id);
        assert_eq!(stored.status, GrantEventStatus::Applied);
        assert_eq!(stored.attempts, 3);

        let mut wrong_key = incoming.clone();
        wrong_key.dedup_key = "other".into();
        assert!(stored.ensure_replay(&wrong_key).is_err());
    }

    // 验证发布要求正确事件和有效领取令牌；换主后旧令牌失效，已完成的同证据允许重放。
    #[test]
    fn lease_takeover_rejects_old_worker_but_applied_replay_is_safe() {
        let mut event = GrantEvent::new(evidence(payloads().remove(0))).unwrap();
        assert!(event.ensure_claim(&event).is_err());
        event.status = GrantEventStatus::Processing;
        assert!(event.ensure_claim(&event).is_err());
        event.lease_owner = Some("worker-a-token".into());
        let claim = event.clone();
        event.ensure_claim(&claim).unwrap();

        let mut other_record = claim.clone();
        other_record.base.id = next_id();
        assert!(event.ensure_claim(&other_record).is_err());

        event.lease_owner = Some("worker-b-token".into());
        assert!(event.ensure_claim(&claim).is_err());
        event.status = GrantEventStatus::Failed;
        assert!(event.ensure_claim(&claim).is_err());

        event.status = GrantEventStatus::Applied;
        event.lease_owner = None;
        event.ensure_claim(&claim).unwrap();
        assert!(event.ensure_claim(&other_record).is_err());

        event.evidence.occurred_at += 1;
        assert!(event.ensure_claim(&claim).is_err());
    }
    // 验证领取次数增加时退避按指数增长，达到上限后即使计数极大也保持 900 秒。
    #[test]
    fn retry_backoff_is_bounded_even_after_many_failures() {
        let mut event = GrantEvent::new(evidence(payloads().remove(0))).unwrap();
        for (attempts, expected) in [(1, 2), (2, 4), (9, 512), (10, 900), (u32::MAX, 900)] {
            event.attempts = attempts;
            assert_eq!(event.retry_delay(), expected);
        }
    }
}
