//! W29 受控关闭的事务内重读、原领域校验和写入。

use super::map_service;
use crate::errors::Error;
use application_core::CommandFingerprint;
use async_trait::async_trait;
use erp_core::common::time::Instant;
use erp_integration::entity::integration_ops::{
    ErrorClass, ErrorTaskStatus, IntegrationErrorTask, ReconciliationDifferenceId,
    ReconciliationDifferenceResolution, ReconciliationDifferenceResolutionId, ResolutionType,
    W29CloseDecision,
};
use erp_integration::repository::IntegrationOpsExt;
use erp_workflow::entity::work_item::WorkItem;
use erp_workflow::ports::W29CloseFact;
use erp_workflow::{Error as WorkflowError, Result as WorkflowResult, WorkItemExt};
use mongodb::Database;
use persistence_core::Executor;

pub(super) fn prepare_w29_close(
    reason_code: &str,
    comment: Option<&str>,
    replacement_work_item_id: Option<&str>,
) -> WorkflowResult<W29CloseFact> {
    let decision = W29CloseDecision::new(reason_code, comment, replacement_work_item_id)
        .map_err(|error| WorkflowError::ValidationError(error.to_string()))?;
    Ok(W29CloseFact {
        close_reason: decision.close_reason().to_string(),
        replacement_work_item_id: decision.replacement_work_item_id().map(str::to_string),
    })
}

pub(super) struct W29CloseInput<'a> {
    pub item: &'a WorkItem,
    pub decision: &'a W29CloseFact,
    pub evidence_reference: &'a str,
    pub actor_id: &'a str,
    pub receipt_id: &'a str,
    pub closed_at: Instant,
}
#[async_trait]
trait W29ClosePort: Sync {
    async fn replacement(&self, id: &str, executor: &mut dyn Executor) -> WorkflowResult<Option<WorkItem>>;
    async fn error_task(
        &self,
        id: &str,
        executor: &mut dyn Executor,
    ) -> WorkflowResult<Option<IntegrationErrorTask>>;
    async fn persist_task(
        &self,
        task: &mut IntegrationErrorTask,
        executor: &mut dyn Executor,
    ) -> WorkflowResult<()>;
    async fn difference(&self, id: &str, executor: &mut dyn Executor) -> WorkflowResult<Option<()>>;
    async fn latest(
        &self,
        id: &ReconciliationDifferenceId,
        executor: &mut dyn Executor,
    ) -> WorkflowResult<Option<ReconciliationDifferenceResolution>>;
    async fn persist_resolution(
        &self,
        resolution: &ReconciliationDifferenceResolution,
        executor: &mut dyn Executor,
    ) -> WorkflowResult<()>;
}
struct MongoW29Close<'a> {
    db: &'a Database,
}
#[async_trait]
impl W29ClosePort for MongoW29Close<'_> {
    async fn replacement(&self, id: &str, executor: &mut dyn Executor) -> WorkflowResult<Option<WorkItem>> {
        self.db
            .work_items()
            .find_work_item(id, executor)
            .await
            .map_err(WorkflowError::from)
    }
    async fn error_task(
        &self,
        id: &str,
        executor: &mut dyn Executor,
    ) -> WorkflowResult<Option<IntegrationErrorTask>> {
        self.db
            .integration_error_tasks()
            .find_work_item_integration_error_task(id, executor)
            .await
            .map_err(WorkflowError::from)
    }
    async fn persist_task(
        &self,
        task: &mut IntegrationErrorTask,
        executor: &mut dyn Executor,
    ) -> WorkflowResult<()> {
        self.db
            .integration_error_tasks()
            .update(task, executor)
            .await
            .map_err(WorkflowError::from)?;
        Ok(())
    }
    async fn difference(&self, id: &str, executor: &mut dyn Executor) -> WorkflowResult<Option<()>> {
        Ok(self
            .db
            .reconciliation_differences()
            .find_work_item_reconciliation_difference(id, executor)
            .await
            .map_err(WorkflowError::from)?
            .map(|_| ()))
    }
    async fn latest(
        &self,
        id: &ReconciliationDifferenceId,
        executor: &mut dyn Executor,
    ) -> WorkflowResult<Option<ReconciliationDifferenceResolution>> {
        self.db
            .reconciliation_difference_resolutions()
            .find_latest_by_difference(id, executor)
            .await
            .map_err(WorkflowError::from)
    }
    async fn persist_resolution(
        &self,
        resolution: &ReconciliationDifferenceResolution,
        executor: &mut dyn Executor,
    ) -> WorkflowResult<()> {
        self.db
            .reconciliation_difference_resolutions()
            .create(resolution, executor)
            .await
            .map_err(WorkflowError::from)?;
        Ok(())
    }
}
pub(super) async fn persist_w29_close(
    db: &Database,
    input: W29CloseInput<'_>,
    executor: &mut dyn Executor,
) -> WorkflowResult<()> {
    persist(&MongoW29Close { db }, input, executor).await
}
async fn persist(
    port: &impl W29ClosePort,
    input: W29CloseInput<'_>,
    executor: &mut dyn Executor,
) -> WorkflowResult<()> {
    let W29CloseInput {
        item,
        decision,
        evidence_reference,
        actor_id,
        receipt_id,
        closed_at,
    } = input;
    if let Some(replacement_work_item_id) = decision.replacement_work_item_id.as_deref() {
        let replacement = port
            .replacement(replacement_work_item_id, executor)
            .await?
            .ok_or_else(|| WorkflowError::NotFound("替代任务不存在".to_string()))?;
        if !replacement.is_w29_replacement_for(item) {
            return Err(WorkflowError::ConflictError(
                "替代任务必须在关闭事务中仍是同一 W29 对象类别的开放正式任务".to_string(),
            ));
        }
    }
    match item.business_object_type.as_str() {
        "integration_error_task" => {
            let mut task = port
                .error_task(&item.business_object_id, executor)
                .await?
                .ok_or_else(|| WorkflowError::NotFound("集成异常任务不存在".to_string()))?;
            let registered_type = if task.error_class == ErrorClass::ResultUnknown {
                erp_workflow::WorkItemType::IntegrationResultUnknown
            } else {
                erp_workflow::WorkItemType::BusinessException
            };
            if item.work_item_type != registered_type {
                return Err(WorkflowError::ConflictError(
                    "任务类型与集成异常分类不一致，请刷新".to_string(),
                ));
            }
            task.transition(
                ErrorTaskStatus::Closed,
                Some(ResolutionType::Close),
                Some(evidence_reference.to_string()),
                closed_at,
            )
            .map_err(|error| map_service(Error::from(error)))?;
            port.persist_task(&mut task, executor).await?;
            Ok(())
        }
        "reconciliation_difference" => {
            let difference_id = ReconciliationDifferenceId::new(item.business_object_id.clone());
            port.difference(&item.business_object_id, executor)
                .await?
                .ok_or_else(|| WorkflowError::NotFound("对账差异不存在".to_string()))?;
            let latest = port.latest(&difference_id, executor).await?;
            if latest
                .as_ref()
                .is_some_and(|resolution| resolution.resulting_status.is_terminal())
            {
                return Err(WorkflowError::ConflictError(
                    "对账差异已经关闭或形成正式结论".to_string(),
                ));
            }
            let resolution_no = W29CloseDecision::next_resolution_no(
                latest.as_ref().map(|resolution| resolution.resolution_no),
            )
            .map_err(|error| map_service(Error::from(error)))?;
            let resolution_id_digest = CommandFingerprint::from_parts([receipt_id.to_string()]);
            let resolution = ReconciliationDifferenceResolution::new_close_evidence(
                ReconciliationDifferenceResolutionId::new(format!(
                    "w29-close-{}",
                    resolution_id_digest.digest_hex()
                )),
                difference_id,
                resolution_no,
                if decision.replacement_work_item_id.is_some() {
                    erp_integration::entity::integration_ops::ResolutionAction::CloseDuplicate
                } else {
                    erp_integration::entity::integration_ops::ResolutionAction::CloseMisrouted
                },
                erp_integration::entity::integration_ops::W29EvidenceReference::parse(evidence_reference)
                    .map_err(|error| map_service(Error::from(error)))?,
                actor_id.to_string(),
                closed_at,
            )
            .map_err(|error| map_service(Error::from(error)))?;
            port.persist_resolution(&resolution, executor).await?;
            Ok(())
        }
        _ => Err(WorkflowError::BusinessLogicError(
            "只有 W29 登记的异常对象允许受控关闭".to_string(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use erp_core::ids::{IntegrationErrorTaskId, WorkItemId};
    use erp_integration::entity::integration_ops::{
        IntegrationErrorTaskData, ReconciliationDifferenceResolutionData, ResolutionAction, ResultingStatus,
    };
    use erp_workflow::entity::work_item::{AssignmentSource, WorkItemData, WorkItemPriority, WorkItemStatus};
    use erp_workflow::WorkItemType;
    use std::sync::Mutex;

    struct Marker(u64);
    impl Executor for Marker {
        fn session(&mut self) -> Option<&mut mongodb::ClientSession> {
            None
        }
    }
    struct Recorder {
        pointer: usize,
        calls: Mutex<Vec<&'static str>>,
        fail: Option<usize>,
        replacement: Option<WorkItem>,
        task: Option<IntegrationErrorTask>,
        difference: bool,
        latest: Option<ReconciliationDifferenceResolution>,
        written_task: Mutex<Option<IntegrationErrorTask>>,
        written_resolution: Mutex<Option<ReconciliationDifferenceResolution>>,
    }
    impl Recorder {
        fn new(ex: &mut Marker, item: &WorkItem) -> Self {
            assert_eq!(ex.0, 172);
            let mut replacement = item.clone();
            replacement.base.id = "replacement".into();
            Self {
                pointer: ex as *mut Marker as usize,
                calls: Mutex::new(Vec::new()),
                fail: None,
                replacement: Some(replacement),
                task: Some(error_task(ErrorClass::MappingError)),
                difference: true,
                latest: None,
                written_task: Mutex::new(None),
                written_resolution: Mutex::new(None),
            }
        }
        fn record(&self, name: &'static str, ex: &mut dyn Executor) -> WorkflowResult<()> {
            assert_eq!(ex as *mut dyn Executor as *mut () as usize, self.pointer);
            let mut calls = self.calls.lock().unwrap();
            let i = calls.len();
            calls.push(name);
            if self.fail == Some(i) {
                return Err(WorkflowError::ConflictError(format!("provider {i}")));
            }
            Ok(())
        }
    }
    #[async_trait]
    impl W29ClosePort for Recorder {
        async fn replacement(&self, id: &str, ex: &mut dyn Executor) -> WorkflowResult<Option<WorkItem>> {
            assert_eq!(id, "replacement");
            self.record("replacement", ex)?;
            Ok(self.replacement.clone())
        }
        async fn error_task(
            &self,
            id: &str,
            ex: &mut dyn Executor,
        ) -> WorkflowResult<Option<IntegrationErrorTask>> {
            assert_eq!(id, "object-1");
            self.record("task", ex)?;
            Ok(self.task.clone())
        }
        async fn persist_task(
            &self,
            task: &mut IntegrationErrorTask,
            ex: &mut dyn Executor,
        ) -> WorkflowResult<()> {
            self.record("task CAS", ex)?;
            *self.written_task.lock().unwrap() = Some(task.clone());
            Ok(())
        }
        async fn difference(&self, id: &str, ex: &mut dyn Executor) -> WorkflowResult<Option<()>> {
            assert_eq!(id, "object-1");
            self.record("difference", ex)?;
            Ok(self.difference.then_some(()))
        }
        async fn latest(
            &self,
            id: &ReconciliationDifferenceId,
            ex: &mut dyn Executor,
        ) -> WorkflowResult<Option<ReconciliationDifferenceResolution>> {
            assert_eq!(id.as_ref(), "object-1");
            self.record("latest", ex)?;
            Ok(self.latest.clone())
        }
        async fn persist_resolution(
            &self,
            resolution: &ReconciliationDifferenceResolution,
            ex: &mut dyn Executor,
        ) -> WorkflowResult<()> {
            self.record("resolution insert", ex)?;
            *self.written_resolution.lock().unwrap() = Some(resolution.clone());
            Ok(())
        }
    }
    fn item(object_type: &str, kind: WorkItemType) -> WorkItem {
        WorkItem::new_at(
            WorkItemId::new("work-item-1"),
            WorkItemData {
                work_item_type: kind,
                business_object_type: object_type.into(),
                business_object_id: "object-1".into(),
                subject_version: "1".into(),
                owner_role: "operations".into(),
                owner_organization_id: "company".into(),
                owner_user_id: "operator".into(),
                assignment_source: AssignmentSource::SystemRule,
                priority: WorkItemPriority::High,
                due_at: None,
                reason_code: None,
                impact_summary: None,
            },
            Instant::from_unix_secs(1),
        )
        .unwrap()
    }
    fn error_task(class: ErrorClass) -> IntegrationErrorTask {
        IntegrationErrorTask::new(
            IntegrationErrorTaskId::new("object-1"),
            IntegrationErrorTaskData {
                message_id: None,
                business_object_id: Some("source-1".into()),
                error_class: class,
                owner_role: None,
                owner_user_id: None,
            },
        )
        .unwrap()
    }
    fn latest(no: u32, action: ResolutionAction) -> ReconciliationDifferenceResolution {
        ReconciliationDifferenceResolution::new(
            ReconciliationDifferenceResolutionId::new("prior"),
            ReconciliationDifferenceResolutionData {
                reconciliation_difference_id: ReconciliationDifferenceId::new("object-1"),
                resolution_no: no,
                resolution_action: action,
                resulting_status: action.derived_status(),
                evidence_reference: Some("prior-evidence".into()),
                handled_by: "previous".into(),
                handled_at: Instant::from_unix_secs(100),
            },
        )
        .unwrap()
    }
    fn input<'a>(item: &'a WorkItem, decision: &'a W29CloseFact) -> W29CloseInput<'a> {
        W29CloseInput {
            item,
            decision,
            evidence_reference: if decision.replacement_work_item_id.is_some() {
                "work_item:work-item-1;replacement_work_item:replacement;audit_log:receipt-1"
            } else {
                "work_item:work-item-1;audit_log:receipt-1"
            },
            actor_id: "operator",
            receipt_id: "receipt-1",
            closed_at: Instant::from_unix_secs(200),
        }
    }
    #[tokio::test]
    async fn w29_error_task_preserves_all_classes_and_original_close_time() {
        for class in [
            ErrorClass::CapabilityGap,
            ErrorClass::MappingError,
            ErrorClass::BusinessRejected,
            ErrorClass::TransientFailure,
            ErrorClass::ResultUnknown,
            ErrorClass::AuthSignature,
            ErrorClass::RateLimited,
            ErrorClass::OutOfOrder,
        ] {
            let kind = if class == ErrorClass::ResultUnknown {
                WorkItemType::IntegrationResultUnknown
            } else {
                WorkItemType::BusinessException
            };
            let item = item("integration_error_task", kind);
            let mut ex = Marker(172);
            let mut port = Recorder::new(&mut ex, &item);
            port.task = Some(error_task(class));
            let decision = prepare_w29_close("MISROUTED", Some("误派"), None).unwrap();
            persist(&port, input(&item, &decision), &mut ex).await.unwrap();
            assert_eq!(*port.calls.lock().unwrap(), ["task", "task CAS"]);
            let written = port.written_task.lock().unwrap();
            let task = written.as_ref().unwrap();
            assert_eq!(task.status, ErrorTaskStatus::Closed);
            assert_eq!(task.resolution_type, Some(ResolutionType::Close));
            assert_eq!(task.resolved_at, Some(Instant::from_unix_secs(200)));
            assert_eq!(
                task.resolution.as_deref(),
                Some("work_item:work-item-1;audit_log:receipt-1")
            );
            assert_eq!(task.base, port.task.as_ref().unwrap().base);
        }
    }
    #[tokio::test]
    async fn w29_resolution_preserves_receipt_id_sequence_and_both_decisions() {
        for duplicate in [false, true] {
            let item = item("reconciliation_difference", WorkItemType::BusinessException);
            let mut ex = Marker(172);
            let mut port = Recorder::new(&mut ex, &item);
            port.latest = Some(latest(7, ResolutionAction::QueryOriginalResult));
            let decision = if duplicate {
                prepare_w29_close("DUPLICATE", None, Some("replacement"))
            } else {
                prepare_w29_close("MISROUTED", Some("误派"), None)
            }
            .unwrap();
            persist(&port, input(&item, &decision), &mut ex).await.unwrap();
            let expected = if duplicate {
                vec!["replacement", "difference", "latest", "resolution insert"]
            } else {
                vec!["difference", "latest", "resolution insert"]
            };
            assert_eq!(*port.calls.lock().unwrap(), expected);
            let written = port.written_resolution.lock().unwrap();
            let resolution = written.as_ref().unwrap();
            assert_eq!(
                resolution.base.id,
                "w29-close-c9ad820b2d5a74d5da3454f5459a3203bc7aca23b5efba8b237ac099e2f7818f"
            );
            assert_eq!(resolution.resolution_no, 8);
            assert_eq!(resolution.handled_at, Instant::from_unix_secs(200));
            assert_eq!(resolution.handled_by, "operator");
            assert_eq!(resolution.resulting_status, ResultingStatus::Closed);
            assert_eq!(
                resolution.resolution_action,
                if duplicate {
                    ResolutionAction::CloseDuplicate
                } else {
                    ResolutionAction::CloseMisrouted
                }
            );
        }
    }
    #[tokio::test]
    async fn w29_stops_on_each_real_provider_failure_with_same_executor() {
        for object_type in ["integration_error_task", "reconciliation_difference"] {
            let expected = if object_type == "integration_error_task" {
                vec!["replacement", "task", "task CAS"]
            } else {
                vec!["replacement", "difference", "latest", "resolution insert"]
            };
            for fail in 0..expected.len() {
                let item = item(object_type, WorkItemType::BusinessException);
                let mut ex = Marker(172);
                let mut port = Recorder::new(&mut ex, &item);
                port.fail = Some(fail);
                let decision = prepare_w29_close("DUPLICATE", None, Some("replacement")).unwrap();
                assert!(
                    matches!(persist(&port, input(&item, &decision), &mut ex).await, Err(WorkflowError::ConflictError(message)) if message == format!("provider {fail}"))
                );
                assert_eq!(*port.calls.lock().unwrap(), expected[..=fail]);
                assert!(port.written_task.lock().unwrap().is_none());
                assert!(port.written_resolution.lock().unwrap().is_none());
            }
        }
    }
    #[tokio::test]
    async fn w29_replacement_and_domain_guards_stop_before_mutation() {
        let decision = prepare_w29_close("DUPLICATE", None, Some("replacement")).unwrap();
        for case in 0..4 {
            let item = item("integration_error_task", WorkItemType::BusinessException);
            let mut ex = Marker(172);
            let mut port = Recorder::new(&mut ex, &item);
            match case {
                0 => port.replacement = None,
                1 => port.replacement.as_mut().unwrap().status = WorkItemStatus::Completed,
                2 => port.task = None,
                _ => port.task = Some(error_task(ErrorClass::ResultUnknown)),
            }
            let error = persist(&port, input(&item, &decision), &mut ex)
                .await
                .unwrap_err();
            assert!(matches!(
                error,
                WorkflowError::NotFound(_) | WorkflowError::ConflictError(_)
            ));
            assert_eq!(port.calls.lock().unwrap().len(), if case < 2 { 1 } else { 2 });
            assert!(port.written_task.lock().unwrap().is_none());
        }
        let decision = prepare_w29_close("MISROUTED", Some("误派"), None).unwrap();
        for case in 0..5 {
            let item = item("reconciliation_difference", WorkItemType::BusinessException);
            let mut ex = Marker(172);
            let mut port = Recorder::new(&mut ex, &item);
            match case {
                0 => port.difference = false,
                1 => port.latest = Some(latest(1, ResolutionAction::ConfirmNoError)),
                2 => port.latest = Some(latest(1, ResolutionAction::ConfirmValidDifference)),
                3 => port.latest = Some(latest(1, ResolutionAction::CloseMisrouted)),
                _ => port.latest = Some(latest(u32::MAX, ResolutionAction::QueryOriginalResult)),
            }
            assert!(persist(&port, input(&item, &decision), &mut ex).await.is_err());
            assert_eq!(port.calls.lock().unwrap().len(), if case == 0 { 1 } else { 2 });
            assert!(port.written_resolution.lock().unwrap().is_none());
        }
        let item = item("unsupported", WorkItemType::BusinessException);
        let mut ex = Marker(172);
        let port = Recorder::new(&mut ex, &item);
        assert!(
            matches!(persist(&port, input(&item, &decision), &mut ex).await, Err(WorkflowError::BusinessLogicError(message)) if message == "只有 W29 登记的异常对象允许受控关闭")
        );
        assert!(port.calls.lock().unwrap().is_empty());
    }
    #[tokio::test]
    async fn w29_invalid_evidence_is_rejected_after_original_reads() {
        let item = item("reconciliation_difference", WorkItemType::BusinessException);
        let mut ex = Marker(172);
        let port = Recorder::new(&mut ex, &item);
        let decision = prepare_w29_close("MISROUTED", Some("误派"), None).unwrap();
        let mut command = input(&item, &decision);
        command.evidence_reference = "invalid";
        assert!(persist(&port, command, &mut ex).await.is_err());
        assert_eq!(*port.calls.lock().unwrap(), ["difference", "latest"]);
        assert!(port.written_resolution.lock().unwrap().is_none());
    }
}
