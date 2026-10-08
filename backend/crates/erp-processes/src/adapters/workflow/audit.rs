//! 将工作流安全业务事件装配到审计域，复用原事务执行器。

use application_core::AuditActor;
use async_trait::async_trait;
use erp_audit::{
    AuditAction, AuditActorLogs, AuditAttemptResult, AuditCode, AuditFact, AuditField, AuditFieldKind,
    AuditLog, AuditValue, BusinessEventContent, BusinessEventContext, BusinessEventResult,
};
use erp_core::AccountKind;
use erp_workflow::Result as WorkflowResult;
use erp_workflow::ports::{
    PreparedWorkflowAudit, WorkflowAuditAttemptResult, WorkflowAuditOperation, WorkflowAuditPort,
};
use mongodb::Database;
use persistence_core::Executor;

use super::map_service;
use crate::audit::{
    AuditAttemptSink, AuditEventSink, AuditedCommand, AuditedWrite, MongoAuditAttemptSink, execute_audited,
    persist_log,
};
use crate::{Error, Result};

const CLOSED: &[AuditCode] = &[AuditCode { code: "CLOSED", label: "已关闭" }];
const OWNER_FIELDS: &[AuditField] = &[
    AuditField { code: "owner", label: "责任人", kind: AuditFieldKind::Changed },
    AuditField { code: "reason", label: "转交原因", kind: AuditFieldKind::Changed },
];
const CLOSE_FIELDS: &[AuditField] =
    &[AuditField { code: "status", label: "任务状态", kind: AuditFieldKind::Code(CLOSED) }];

/// 将事务前工作流输入映射为静态动作；禁止把说明正文复制到结构化事实。
fn event_context(audit: &PreparedWorkflowAudit) -> Result<BusinessEventContext> {
    let (code, resource_type, label, fields) = match audit.operation {
        Some(WorkflowAuditOperation::Reassigned) => {
            ("work_item.reassign", "work_item", "转交任务责任", OWNER_FIELDS)
        },
        Some(WorkflowAuditOperation::PurchaseOwnerReassigned) => {
            ("purchase_order.owner_reassign", "purchase_order", "转交采购履约责任", OWNER_FIELDS)
        },
        Some(WorkflowAuditOperation::Closed) => {
            ("work_item.close", "work_item", "关闭异常任务", CLOSE_FIELDS)
        },
        None => return Err(Error::ValidationError("工作流业务事件未登记".to_string())),
    };
    if audit.action != code || audit.resource_type != resource_type || audit.command_id.is_none() {
        return Err(Error::ValidationError("工作流业务事件与命令不一致".to_string()));
    }
    let kind =
        AccountKind::parse(&audit.actor_type).map_err(|error| Error::ValidationError(error.to_string()))?;
    Ok(BusinessEventContext::new(
        AuditActor::new(audit.actor_id.clone(), audit.actor_account.clone(), kind),
        AuditAction { code, resource_type, label, version: 1, allowed_fields: fields },
    )?
    .with_actor_name_snapshot(audit.actor_name_snapshot.clone())?
    .with_request_id(audit.request_id.clone())?
    .with_event_sequence(audit.event_sequence.get())?
    .with_command_id(audit.command_id.clone())?)
}

/// 按领域已冻结的上下文构造普通工作流事件，保留原事件序号。
/// # 参数
/// * `audit` - 原调用方准备的安全事件输入。
/// # 返回
/// 返回保留事件编号、名称、请求及顺序的结构化日志。
/// # 错误
/// 身份、动作或上下文非法时返回原错误分类。
fn resource_log_from_prepared(audit: &PreparedWorkflowAudit) -> Result<AuditLog> {
    let actor_type =
        AccountKind::parse(&audit.actor_type).map_err(|error| Error::ValidationError(error.to_string()))?;
    let actor = AuditActor::new(audit.actor_id.clone(), audit.actor_account.clone(), actor_type)
        .with_actor_name_snapshot(audit.actor_name_snapshot.clone())
        .and_then(|actor| actor.with_request_id(audit.request_id.clone()))
        .map_err(|error| Error::ValidationError(error.to_string()))?;
    Ok(actor
        .resource_log_with_id(
            audit.id.clone(),
            &audit.action,
            &audit.resource_type,
            audit.resource_id.clone(),
            audit.message.clone(),
        )?
        .with_event_sequence(audit.event_sequence.get())?)
}

fn event_content(audit: &PreparedWorkflowAudit) -> BusinessEventContent {
    let facts = match audit.operation {
        Some(WorkflowAuditOperation::Closed) => vec![AuditFact {
            field: "status".to_string(),
            value: AuditValue::Code { code: "CLOSED".to_string(), label: "已关闭".to_string() },
        }],
        _ => vec![
            AuditFact { field: "owner".to_string(), value: AuditValue::Changed },
            AuditFact { field: "reason".to_string(), value: AuditValue::Changed },
        ],
    };
    BusinessEventContent {
        target_id: audit.resource_id.clone(),
        target_number: None,
        result: BusinessEventResult::Succeeded,
        field_changes: Vec::new(),
        facts,
    }
}

struct WorkflowEventCommand<'a>(&'a PreparedWorkflowAudit);
#[async_trait]
impl AuditedCommand for WorkflowEventCommand<'_> {
    type Output = ();
    async fn execute(&self, _executor: &mut dyn Executor) -> Result<AuditedWrite<()>> {
        Ok(AuditedWrite::Fresh { result: (), content: event_content(self.0) })
    }
}

struct WorkflowEventSink<'a> {
    db: &'a Database,
    event_id: &'a str,
}
#[async_trait]
impl AuditEventSink for WorkflowEventSink<'_> {
    async fn persist(&self, log: &AuditLog, executor: &mut dyn Executor) -> Result<()> {
        let mut log = log.clone();
        log.base.id = self.event_id.to_string();
        persist_log(self.db, &log, executor).await?;
        Ok(())
    }
}

/// 经 `erp-audit` 持久化工作流审计。
#[derive(Clone)]
pub struct WorkflowAudit {
    db: Database,
}
impl WorkflowAudit {
    /// 绑定业务事务所在数据库。
    ///
    /// # 参数
    /// * `db` - 原用例数据库。
    ///
    /// # 返回
    /// 返回同事务审计 adapter。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn new(db: Database) -> Self {
        Self { db }
    }
}

#[async_trait]
impl WorkflowAuditPort for WorkflowAudit {
    fn validate(&self, audit: &PreparedWorkflowAudit) -> WorkflowResult<()> {
        event_context(audit).map(|_| ()).map_err(map_service)
    }

    async fn persist_attempt(
        &self,
        audit: &PreparedWorkflowAudit,
        result: WorkflowAuditAttemptResult,
    ) -> WorkflowResult<()> {
        let result = match result {
            WorkflowAuditAttemptResult::Failed => AuditAttemptResult::Failed,
            WorkflowAuditAttemptResult::Rejected => AuditAttemptResult::Rejected,
            WorkflowAuditAttemptResult::Unknown => AuditAttemptResult::Unknown,
        };
        let context = event_context(audit)
            .map_err(map_service)?
            .with_target(Some(audit.resource_id.clone()), None)
            .map_err(|error| map_service(error.into()))?;
        MongoAuditAttemptSink::new(&self.db)
            .persist_attempt(&context.attempt(result))
            .await
            .map_err(map_service)?;
        Ok(())
    }

    async fn persist(
        &self,
        audit: &PreparedWorkflowAudit,
        executor: &mut dyn Executor,
    ) -> WorkflowResult<()> {
        if audit.operation.is_some() {
            let context = event_context(audit).map_err(map_service)?;
            return execute_audited(
                &context,
                &WorkflowEventSink { db: &self.db, event_id: &audit.id },
                executor,
                &WorkflowEventCommand(audit),
            )
            .await
            .map_err(map_service);
        }
        let log = resource_log_from_prepared(audit).map_err(map_service)?;
        persist_log(&self.db, &log, executor).await.map_err(|error| map_service(Error::from(error)))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU32;

    use super::*;
    fn prepared(operation: WorkflowAuditOperation) -> PreparedWorkflowAudit {
        let (action, resource) = match operation {
            WorkflowAuditOperation::Reassigned => ("work_item.reassign", "work_item"),
            WorkflowAuditOperation::PurchaseOwnerReassigned => {
                ("purchase_order.owner_reassign", "purchase_order")
            },
            WorkflowAuditOperation::Closed => ("work_item.close", "work_item"),
        };
        PreparedWorkflowAudit::resource_with_message(
            AuditActor::new("actor".into(), "account".into(), AccountKind::Admin),
            action,
            resource,
            "object".into(),
            Some("密钥、接收人、完整说明不得展示".into()),
        )
        .unwrap()
        .for_command("command".into(), operation)
        .unwrap()
    }
    #[test]
    fn workflow_event_records_chinese_safe_change_markers_and_command_reference() {
        for operation in [
            WorkflowAuditOperation::Reassigned,
            WorkflowAuditOperation::PurchaseOwnerReassigned,
            WorkflowAuditOperation::Closed,
        ] {
            let audit = prepared(operation);
            let log = event_context(&audit).unwrap().log(event_content(&audit)).unwrap();
            assert!(!log.message.as_deref().unwrap().contains("密钥"));
            let event = log.structured_event.unwrap();
            assert_eq!(event.command_id.as_deref(), Some("command"));
            assert_eq!(event.result, BusinessEventResult::Succeeded);
            assert_eq!(event.actor_name_snapshot, None);
            assert!(event.action_label.chars().any(|value| ('\u{4e00}'..='\u{9fff}').contains(&value)));
        }
    }
    #[test]
    fn workflow_event_rejects_mismatched_static_metadata_before_write() {
        let mut audit = prepared(WorkflowAuditOperation::Closed);
        audit.action = "work_item.reassign".into();
        assert!(event_context(&audit).is_err());
        audit = prepared(WorkflowAuditOperation::Closed);
        audit.actor_id.clear();
        assert!(event_context(&audit).is_err());
    }

    /// 工作流准备端口携带本次名称，组合适配保留冻结快照和原命令。
    #[test]
    fn workflow_prepared_port_preserves_current_actor_name() {
        let actor = AuditActor::new("actor".into(), "account".into(), AccountKind::Admin)
            .with_actor_name_snapshot(Some("  孙立新  ".into()))
            .unwrap()
            .with_request_id(Some("workflow-request".into()))
            .unwrap();
        let prepared =
            PreparedWorkflowAudit::resource(actor, "work_item.close", "work_item", "item-1".into())
                .unwrap()
                .for_command("close-command".into(), WorkflowAuditOperation::Closed)
                .unwrap();
        let log = event_context(&prepared).unwrap().log(event_content(&prepared)).unwrap();
        let event = log.structured_event.unwrap();
        assert_eq!(event.actor_name_snapshot.as_deref(), Some("孙立新"));
        assert_eq!(event.command_id.as_deref(), Some("close-command"));
        assert_eq!(event.resource_id, "item-1");
        assert_eq!(event.request_id.as_deref(), Some("workflow-request"));
        assert_eq!(
            event_context(&prepared).unwrap().attempt(AuditAttemptResult::Unknown).request_id.as_deref(),
            Some("workflow-request")
        );
    }

    #[test]
    fn workflow_typed_context_preserves_positive_order_and_frozen_request() {
        for operation in [
            WorkflowAuditOperation::Reassigned,
            WorkflowAuditOperation::PurchaseOwnerReassigned,
            WorkflowAuditOperation::Closed,
        ] {
            let mut audit = prepared(operation).with_event_sequence(NonZeroU32::new(7).unwrap());
            audit.actor_name_snapshot = Some("发生时名称".into());
            audit.request_id = Some("request-original".into());
            let log = event_context(&audit).unwrap().log(event_content(&audit)).unwrap();
            let event = log.structured_event.as_ref().unwrap();
            assert_eq!(event.event_sequence.get(), 7);
            assert_eq!(event.command_id, audit.command_id);
            assert_eq!(event.actor_name_snapshot.as_deref(), Some("发生时名称"));
            assert_eq!(event.request_id.as_deref(), Some("request-original"));
            assert!(!log.message.as_deref().unwrap().contains("request-original"));
            assert!(!log.message.as_deref().unwrap().contains("密钥"));
        }
    }

    #[test]
    fn workflow_ordinary_adapter_preserves_original_id_context_and_sequence() {
        for request_id in [Some("request-original".to_string()), None] {
            let actor = AuditActor::new("actor".into(), "account".into(), AccountKind::Admin)
                .with_actor_name_snapshot(Some("发生时名称".into()))
                .unwrap()
                .with_request_id(request_id.clone())
                .unwrap();
            let prepared = PreparedWorkflowAudit::resource_with_id(
                "original-event".into(),
                actor.clone(),
                "approval_definition.create_draft",
                "approval_process_definition",
                "definition-1".into(),
                Some("token=private-token;body=private-body".into()),
            )
            .unwrap()
            .with_event_sequence(NonZeroU32::new(9).unwrap());
            let changed = actor.with_request_id(Some("request-current".into())).unwrap();
            assert_eq!(changed.request_id(), Some("request-current"));
            let log = resource_log_from_prepared(&prepared).unwrap();
            let event = log.structured_event.as_ref().unwrap();
            assert_eq!(log.base.id, "original-event");
            assert_eq!(event.occurred_at, log.base.created_at);
            assert_eq!(event.actor_name_snapshot.as_deref(), Some("发生时名称"));
            assert_eq!(event.request_id, request_id);
            assert_eq!(event.event_sequence.get(), 9);
            let serialized = serde_json::to_string(&log).unwrap();
            assert!(!serialized.contains("private-token"));
            assert!(!serialized.contains("private-body"));
        }
    }
}
