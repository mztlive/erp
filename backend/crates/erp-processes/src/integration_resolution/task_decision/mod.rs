//! W29 强类型任务动作、任务完成与无任务直接对账决定。

mod action;
mod complete;
mod direct;
mod execution;
mod guard;
#[cfg(test)]
mod tests;

use application_core::AuditActor;
use erp_audit::{
    AuditAction, AuditCode, AuditFact, AuditField, AuditFieldKind, AuditValue, BusinessEventContent,
    BusinessEventContext, BusinessEventResult,
};
use erp_integration::dto::IntegrationActionOutcome;
use erp_integration::entity::integration_ops::{
    IntegrationCommandIdentity, IntegrationCommandReceipt, IntegrationCommandResult,
    IntegrationReceiptPayload,
};
use erp_integration::repository::IntegrationCommandExt;
use mongodb::Database;
use persistence_core::Executor;

#[cfg(test)]
use self::guard::command_identity;
use crate::audit::{AuditEventSink, AuditedCommand, AuditedWrite, MongoAuditEventSink, execute_audited};
use crate::{Error, Result};

const TASK_ACTION_AUDIT: &str = "integration.task_action";
const TASK_COMPLETION_AUDIT: &str = "integration.task_completion";
const DIRECT_DECISION_AUDIT: &str = "integration.direct_reconciliation";
const OUTCOMES: &[AuditCode] = &[
    AuditCode { code: "TERMINAL_EVIDENCE_FOUND", label: "已找到终态证据" },
    AuditCode { code: "NO_RESULT_CONFIRMED", label: "已确认原操作无结果" },
    AuditCode { code: "RESULT_UNKNOWN", label: "结果仍未知" },
    AuditCode { code: "REPLAY_ACCEPTED", label: "权威操作已受理重放" },
    AuditCode { code: "REATTRIBUTED", label: "已重新归集" },
    AuditCode { code: "EVIDENCE_LINKED", label: "已关联正式补偿" },
    AuditCode { code: "EVIDENCE_ADDED", label: "已追加证据" },
    AuditCode { code: "CONFIRMED_NO_ERROR", label: "已确认无误" },
    AuditCode { code: "CONFIRMED_VALID_DIFFERENCE", label: "已确认有效差异" },
];

fn audit_context(actor: &AuditActor, identity: &IntegrationCommandIdentity) -> Result<BusinessEventContext> {
    let (resource_type, label) = match identity.action() {
        TASK_ACTION_AUDIT => ("work_item", "处理集成异常任务"),
        TASK_COMPLETION_AUDIT => ("work_item", "完成集成异常任务"),
        DIRECT_DECISION_AUDIT => ("reconciliation_difference", "决定对账差异"),
        _ => return Err(Error::ValidationError("W29 审计动作未登记".to_string())),
    };
    Ok(BusinessEventContext::new(
        actor.clone(),
        AuditAction {
            code: match identity.action() {
                TASK_ACTION_AUDIT => TASK_ACTION_AUDIT,
                TASK_COMPLETION_AUDIT => TASK_COMPLETION_AUDIT,
                _ => DIRECT_DECISION_AUDIT,
            },
            resource_type,
            label,
            version: 1,
            allowed_fields: &[AuditField {
                code: "business_outcome",
                label: "业务处理结果",
                kind: AuditFieldKind::Code(OUTCOMES),
            }],
        },
    )?
    .with_command_id(Some(identity.receipt_id().to_string()))?
    .with_target(Some(identity.resource_id().to_string()), None)?)
}

#[async_trait::async_trait]
trait CommandReceiptSink: Send + Sync {
    async fn persist_receipt(
        &self,
        receipt: &IntegrationCommandReceipt,
        executor: &mut dyn Executor,
    ) -> Result<()>;
}
struct MongoCommandReceiptSink<'a>(&'a Database);
#[async_trait::async_trait]
impl CommandReceiptSink for MongoCommandReceiptSink<'_> {
    async fn persist_receipt(
        &self,
        receipt: &IntegrationCommandReceipt,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        self.0.integration_command_receipts().create(receipt, executor).await?;
        Ok(())
    }
}
struct ReceiptWrite<'a, P> {
    sink: &'a P,
    receipt: IntegrationCommandReceipt,
}
#[async_trait::async_trait]
impl<P: CommandReceiptSink> AuditedCommand for ReceiptWrite<'_, P> {
    type Output = ();
    async fn execute(&self, executor: &mut dyn Executor) -> Result<AuditedWrite<()>> {
        self.sink.persist_receipt(&self.receipt, executor).await?;
        let outcome = match &self.receipt.result {
            IntegrationCommandResult::TaskAction(result) => result.outcome,
            IntegrationCommandResult::DirectDecision(result) => result.outcome,
            IntegrationCommandResult::TaskCompletion(_) => IntegrationActionOutcome::TerminalEvidenceFound,
        };
        let label = OUTCOMES
            .iter()
            .find(|value| value.code == outcome.as_str())
            .ok_or_else(|| Error::Internal("W29 业务结果未登记".to_string()))?
            .label;
        Ok(AuditedWrite::Fresh {
            result: (),
            content: BusinessEventContent {
                target_id: self.receipt.resource_id.clone(),
                target_number: None,
                result: BusinessEventResult::Succeeded,
                field_changes: Vec::new(),
                facts: vec![AuditFact {
                    field: "business_outcome".to_string(),
                    value: AuditValue::Code { code: outcome.as_str().to_string(), label: label.to_string() },
                }],
            },
        })
    }
}

async fn store_receipt<T: IntegrationReceiptPayload>(
    db: &Database,
    actor: &AuditActor,
    identity: &IntegrationCommandIdentity,
    result: T,
    executor: &mut dyn Executor,
) -> Result<()> {
    store_receipt_with_sinks(
        actor,
        identity,
        result,
        executor,
        &MongoCommandReceiptSink(db),
        &MongoAuditEventSink::new(db),
    )
    .await
}

async fn store_receipt_with_sinks<T: IntegrationReceiptPayload, P: CommandReceiptSink, S: AuditEventSink>(
    actor: &AuditActor,
    identity: &IntegrationCommandIdentity,
    result: T,
    executor: &mut dyn Executor,
    receipts: &P,
    events: &S,
) -> Result<()> {
    let context = audit_context(actor, identity)?;
    let receipt = IntegrationCommandReceipt::new(
        identity,
        actor.id(),
        result.into_result(),
        context.event_id().to_string(),
    )?;
    execute_audited(&context, events, executor, &ReceiptWrite { sink: receipts, receipt }).await
}

#[cfg(test)]
mod receipt_event_tests {
    use std::sync::Mutex;

    use erp_audit::AuditLog;
    use erp_core::AccountKind;
    use erp_integration::entity::integration_ops::ActionReceiptResult;

    use super::*;

    struct TestExecutor;
    impl Executor for TestExecutor {
        fn session(&mut self) -> Option<&mut mongodb::ClientSession> {
            None
        }
    }
    struct Sinks {
        executor_id: usize,
        fail_receipt: bool,
        fail_event: bool,
        calls: Mutex<Vec<&'static str>>,
        receipts: Mutex<Vec<IntegrationCommandReceipt>>,
        events: Mutex<Vec<AuditLog>>,
    }
    impl Sinks {
        fn visit(&self, executor: &mut dyn Executor, step: &'static str) {
            assert_eq!(executor as *mut dyn Executor as *mut () as usize, self.executor_id);
            self.calls.lock().unwrap().push(step);
        }
    }
    #[async_trait::async_trait]
    impl CommandReceiptSink for Sinks {
        async fn persist_receipt(
            &self,
            receipt: &IntegrationCommandReceipt,
            executor: &mut dyn Executor,
        ) -> Result<()> {
            self.visit(executor, "receipt");
            if self.fail_receipt {
                return Err(Error::Internal("original receipt failure".into()));
            }
            self.receipts.lock().unwrap().push(receipt.clone());
            Ok(())
        }
    }
    #[async_trait::async_trait]
    impl AuditEventSink for Sinks {
        async fn persist(&self, log: &AuditLog, executor: &mut dyn Executor) -> Result<()> {
            self.visit(executor, "event");
            if self.fail_event {
                return Err(Error::Internal("original event failure".into()));
            }
            self.events.lock().unwrap().push(log.clone());
            Ok(())
        }
    }
    fn sink(executor: &mut TestExecutor, fail_receipt: bool, fail_event: bool) -> Sinks {
        Sinks {
            executor_id: executor as *mut TestExecutor as usize,
            fail_receipt,
            fail_event,
            calls: Mutex::new(Vec::new()),
            receipts: Mutex::new(Vec::new()),
            events: Mutex::new(Vec::new()),
        }
    }
    fn actor() -> AuditActor {
        AuditActor::new("actor".into(), "account".into(), AccountKind::Admin)
    }
    fn identity() -> IntegrationCommandIdentity {
        IntegrationCommandIdentity::new(
            "actor",
            TASK_ACTION_AUDIT,
            "work_item",
            "task",
            "raw-secret-key",
            b"complete command",
        )
    }
    fn result() -> ActionReceiptResult {
        ActionReceiptResult {
            outcome: IntegrationActionOutcome::ResultUnknown,
            business_result_reference: None,
            verified_evidence: Vec::new(),
        }
    }
    #[tokio::test]
    async fn w29_receipt_and_event_execute_true_boundary_in_order_with_separate_business_unknown() {
        let mut executor = TestExecutor;
        let sink = sink(&mut executor, false, false);
        store_receipt_with_sinks(&actor(), &identity(), result(), &mut executor, &sink, &sink).await.unwrap();
        assert_eq!(*sink.calls.lock().unwrap(), ["receipt", "event"]);
        let receipts = sink.receipts.lock().unwrap();
        let events = sink.events.lock().unwrap();
        assert_eq!(receipts.len(), 1);
        assert_eq!(events.len(), 1);
        assert_eq!(receipts[0].audit_event_id, events[0].base.id);
        assert_ne!(identity().receipt_id(), events[0].base.id);
        let event = events[0].structured_event.as_ref().unwrap();
        assert_eq!(event.command_id.as_deref(), Some(identity().receipt_id()));
        assert_eq!(event.result, BusinessEventResult::Succeeded);
        assert_eq!(
            event.facts[0].value,
            AuditValue::Code { code: "RESULT_UNKNOWN".into(), label: "结果仍未知".into() }
        );
        assert!(events[0].message.as_deref().unwrap().contains("处理集成异常任务"));
        assert!(!serde_json::to_string(&*events).unwrap().contains("raw-secret-key"));
    }
    #[tokio::test]
    async fn w29_receipt_or_audit_failure_stops_boundary_and_preserves_first_error() {
        for (fail_receipt, fail_event) in [(true, false), (false, true)] {
            let mut executor = TestExecutor;
            let sink = sink(&mut executor, fail_receipt, fail_event);
            let error =
                store_receipt_with_sinks(&actor(), &identity(), result(), &mut executor, &sink, &sink)
                    .await
                    .unwrap_err();
            assert!(
                matches!(error, Error::Internal(message) if message == if fail_receipt { "original receipt failure" } else { "original event failure" })
            );
            assert_eq!(
                *sink.calls.lock().unwrap(),
                if fail_receipt { vec!["receipt"] } else { vec!["receipt", "event"] }
            );
            assert!(sink.events.lock().unwrap().is_empty());
        }
    }
}
