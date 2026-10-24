//! 按启动命令身份回读回执指定的精确实例。

use async_trait::async_trait;
use bpm::ids::ApprovalProcessInstanceId;
use bpm::model::types::ApprovalCommandKind;
use bpm::model::{ApprovalCommandReceipt, ApprovalProcessInstance, IdempotencyKey};
use erp_workflow::BpmExt;
use erp_workflow::entity::approval_integration::ApprovalStartIdentity;
use erp_workflow::service::approval::execution::idempotency::{
    ReceiptBranch, StartIdentityParams, normalize_idempotency_key, payload_conflict_error, start_identity,
};
use mongodb::Database;
use persistence_core::Executor;

use crate::{Error, Result};

/// 各业务入口沿用的冻结事实冲突文案。
pub(crate) struct StartReplayErrors {
    pub missing_instance: &'static str,
    pub identity_mismatch: &'static str,
}

/// 使用调用方执行器回读审批启动结果，保留首个候选命中语义。
///
/// # 参数
/// * `db` - 审批数据库。
/// * `expected` - 入口冻结的预期运行身份。
/// * `idempotency_key` - 调用方原始幂等键。
/// * `errors` - 入口既有冲突文案。
/// * `executor` - 入口授权及恢复查询使用的同一执行器。
///
/// # 返回
/// 无回执返回 `None`；同载荷且冻结身份一致时返回回执指定的实例 ID。
///
/// # 错误
/// 保留幂等键错误、载荷冲突、仓储错误及入口的冻结事实冲突。
pub(crate) async fn replay_start(
    db: &Database,
    expected: ApprovalStartIdentity<'_>,
    idempotency_key: &str,
    errors: StartReplayErrors,
    executor: &mut dyn Executor,
) -> Result<Option<String>> {
    replay_from_store(db, expected, idempotency_key, errors, executor).await
}

#[async_trait]
trait StartReplayStore: Sync {
    async fn receipt(
        &self,
        scope: &str,
        key: &IdempotencyKey,
        executor: &mut dyn Executor,
    ) -> Result<Option<ApprovalCommandReceipt>>;
    async fn instance(
        &self,
        id: &ApprovalProcessInstanceId,
        executor: &mut dyn Executor,
    ) -> Result<Option<ApprovalProcessInstance>>;
}

#[async_trait]
impl StartReplayStore for Database {
    async fn receipt(
        &self,
        scope: &str,
        key: &IdempotencyKey,
        executor: &mut dyn Executor,
    ) -> Result<Option<ApprovalCommandReceipt>> {
        Ok(self
            .bpm_workflow()
            .find_command_receipt(ApprovalCommandKind::StartApproval, scope, key, executor)
            .await?)
    }

    async fn instance(
        &self,
        id: &ApprovalProcessInstanceId,
        executor: &mut dyn Executor,
    ) -> Result<Option<ApprovalProcessInstance>> {
        Ok(self.bpm_workflow().find_instance_by_id(id, executor).await?)
    }
}

async fn replay_from_store(
    store: &impl StartReplayStore,
    expected: ApprovalStartIdentity<'_>,
    idempotency_key: &str,
    errors: StartReplayErrors,
    executor: &mut dyn Executor,
) -> Result<Option<String>> {
    let identity = start_identity(StartIdentityParams {
        idempotency_key: normalize_idempotency_key(idempotency_key)?,
        process_kind: expected.process_kind.as_str(),
        subject_kind: expected.subject.subject_kind(),
        subject_id: expected.subject.subject_id(),
        subject_version: expected.subject_version,
        binding_id: expected.binding.approval_process_definition_id.as_ref(),
        definition_version: expected.binding.approval_definition_version,
        actor_participant_id: expected.actor_id,
    })?;
    let mut receipt = None;
    for scope in identity.scope_candidates() {
        receipt = store.receipt(scope, identity.idempotency_key(), executor).await?;
        if receipt.is_some() {
            break;
        }
    }
    let receipt = match identity.classify(receipt.as_ref()) {
        ReceiptBranch::Fresh => return Ok(None),
        ReceiptBranch::PayloadConflict => return Err(payload_conflict_error().into()),
        ReceiptBranch::SamePayload(receipt) => receipt,
    };
    let instance = store
        .instance(&ApprovalProcessInstanceId::new(&receipt.result_ref), executor)
        .await?
        .ok_or_else(|| Error::ConflictError(errors.missing_instance.to_string()))?;
    if !expected.matches_instance(&instance, &receipt.result_ref) {
        return Err(Error::ConflictError(errors.identity_mismatch.to_string()));
    }
    Ok(Some(instance.base.id))
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Mutex;

    use bpm::ids::{ApprovalCommandReceiptId, ApprovalProcessDefinitionId};
    use bpm::model::{NewProcessInstance, ParticipantId, ProcessKind, SubjectRef, Timestamp};
    use erp_core::common::time::Instant;
    use erp_workflow::ErrorCode;
    use erp_workflow::entity::document_registry::business_document::ApprovalDefinitionBinding;
    use erp_workflow::service::approval::execution::idempotency::{
        PreparedCommandIdentity, legacy_payload_digest,
    };
    use persistence_core::NoTransaction;

    use super::*;

    struct Fixture {
        binding: ApprovalDefinitionBinding,
        subject: SubjectRef,
    }

    impl Fixture {
        fn new() -> Self {
            Self {
                binding: ApprovalDefinitionBinding {
                    approval_process_definition_id: ApprovalProcessDefinitionId::new("definition-1"),
                    approval_definition_version: 2,
                    approval_binding_version: 1,
                    approval_definition_bound_at: Instant::from_unix_secs(1),
                },
                subject: SubjectRef::new("sales_order", "order-1").unwrap(),
            }
        }

        fn expected(&self) -> ApprovalStartIdentity<'_> {
            ApprovalStartIdentity {
                process_kind: ProcessKind::SalesOrder,
                subject: &self.subject,
                subject_version: 3,
                binding: &self.binding,
                actor_id: "actor-1",
            }
        }

        fn identity(&self) -> PreparedCommandIdentity {
            start_identity(StartIdentityParams {
                idempotency_key: normalize_idempotency_key("key-1").unwrap(),
                process_kind: "sales_order",
                subject_kind: "sales_order",
                subject_id: "order-1",
                subject_version: 3,
                binding_id: "definition-1",
                definition_version: 2,
                actor_participant_id: "actor-1",
            })
            .unwrap()
        }

        fn receipt(&self) -> ApprovalCommandReceipt {
            ApprovalCommandReceipt::new(
                ApprovalCommandReceiptId::new("receipt-1"),
                self.identity().current(),
                "original-instance",
                Timestamp::from_unix_secs(1).unwrap(),
            )
            .unwrap()
        }

        fn legacy_receipt(&self) -> ApprovalCommandReceipt {
            let mut receipt = self.receipt();
            receipt.scope_id = self.identity().scope_candidates()[1].to_string();
            receipt.payload_digest = legacy_payload_digest("definition-1\u{1f}2\u{1f}3\u{1f}actor-1");
            receipt
        }

        fn instance(&self) -> ApprovalProcessInstance {
            ApprovalProcessInstance::start_running(NewProcessInstance {
                id: ApprovalProcessInstanceId::new("original-instance"),
                process_definition_id: self.binding.approval_process_definition_id.clone(),
                definition_version: 2,
                process_kind: ProcessKind::SalesOrder,
                subject: self.subject.clone(),
                subject_version: 3,
                started_by: ParticipantId::new("actor-1").unwrap(),
                at: Timestamp::from_unix_secs(1).unwrap(),
            })
            .unwrap()
        }
    }

    struct RecordingStore {
        receipts: HashMap<String, ApprovalCommandReceipt>,
        instance: Option<ApprovalProcessInstance>,
        calls: Mutex<Vec<(String, usize)>>,
        fail_at: Option<&'static str>,
    }

    impl RecordingStore {
        fn new(receipts: Vec<ApprovalCommandReceipt>, instance: Option<ApprovalProcessInstance>) -> Self {
            Self {
                receipts: receipts.into_iter().map(|receipt| (receipt.scope_id.clone(), receipt)).collect(),
                instance,
                calls: Mutex::new(Vec::new()),
                fail_at: None,
            }
        }

        fn record(&self, phase: &str, reference: &str, executor: &mut dyn Executor) -> Result<()> {
            self.calls
                .lock()
                .unwrap()
                .push((format!("{phase}:{reference}"), executor as *mut dyn Executor as *mut () as usize));
            if self.fail_at == Some(phase) {
                return Err(Error::ConflictError("original-read-error".to_string()));
            }
            Ok(())
        }
    }

    #[async_trait]
    impl StartReplayStore for RecordingStore {
        async fn receipt(
            &self,
            scope: &str,
            key: &IdempotencyKey,
            executor: &mut dyn Executor,
        ) -> Result<Option<ApprovalCommandReceipt>> {
            assert_eq!(key.as_str(), "key-1");
            self.record("receipt", scope, executor)?;
            Ok(self.receipts.get(scope).cloned())
        }

        async fn instance(
            &self,
            id: &ApprovalProcessInstanceId,
            executor: &mut dyn Executor,
        ) -> Result<Option<ApprovalProcessInstance>> {
            self.record("instance", id.as_ref(), executor)?;
            Ok(self.instance.clone())
        }
    }

    fn errors() -> StartReplayErrors {
        StartReplayErrors {
            missing_instance: "original-missing-instance",
            identity_mismatch: "original-mismatch",
        }
    }

    #[tokio::test]
    async fn missing_receipt_queries_v3_then_legacy_and_never_reads_an_instance() {
        let fixture = Fixture::new();
        let store = RecordingStore::new(Vec::new(), Some(fixture.instance()));
        let result = replay_from_store(&store, fixture.expected(), " key-1 ", errors(), &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(result, None);
        assert_eq!(
            store.calls.lock().unwrap().iter().map(|(call, _)| call.clone()).collect::<Vec<_>>(),
            fixture
                .identity()
                .scope_candidates()
                .iter()
                .map(|scope| format!("receipt:{scope}"))
                .collect::<Vec<_>>()
        );
    }

    #[tokio::test]
    async fn current_receipt_wins_and_reads_its_exact_instance_with_the_same_executor() {
        let fixture = Fixture::new();
        let mut legacy = fixture.legacy_receipt();
        legacy.result_ref = "another-instance".into();
        let store = RecordingStore::new(vec![fixture.receipt(), legacy], Some(fixture.instance()));
        let mut executor = NoTransaction;
        let executor_id = &mut executor as *mut NoTransaction as usize;
        let result =
            replay_from_store(&store, fixture.expected(), "key-1", errors(), &mut executor).await.unwrap();
        assert_eq!(result.as_deref(), Some("original-instance"));
        assert_eq!(
            *store.calls.lock().unwrap(),
            vec![
                (format!("receipt:{}", fixture.identity().scope_candidates()[0]), executor_id),
                ("instance:original-instance".into(), executor_id),
            ]
        );
    }

    #[tokio::test]
    async fn legacy_receipt_replays_when_current_scope_is_absent() {
        let fixture = Fixture::new();
        let store = RecordingStore::new(vec![fixture.legacy_receipt()], Some(fixture.instance()));
        let result = replay_from_store(&store, fixture.expected(), "key-1", errors(), &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(result.as_deref(), Some("original-instance"));
        assert_eq!(store.calls.lock().unwrap().len(), 3);
    }

    #[tokio::test]
    async fn conflicting_current_receipt_does_not_fall_back_to_matching_legacy() {
        let fixture = Fixture::new();
        let mut current = fixture.receipt();
        current.payload_digest = "different-payload".into();
        let store = RecordingStore::new(vec![current, fixture.legacy_receipt()], Some(fixture.instance()));
        let error = replay_from_store(&store, fixture.expected(), "key-1", errors(), &mut NoTransaction)
            .await
            .unwrap_err();
        assert!(matches!(error, Error::Coded(ErrorCode::ApprovalIdempotencyPayloadConflict)));
        assert_eq!(store.calls.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn missing_or_mismatched_instance_preserves_the_callers_conflict() {
        let fixture = Fixture::new();
        let mut changed = fixture.instance();
        changed.subject_version += 1;
        for (instance, expected_message) in
            [(None, "original-missing-instance"), (Some(changed), "original-mismatch")]
        {
            let store = RecordingStore::new(vec![fixture.receipt()], instance);
            let error = replay_from_store(&store, fixture.expected(), "key-1", errors(), &mut NoTransaction)
                .await
                .unwrap_err();
            assert!(matches!(error, Error::ConflictError(message) if message == expected_message));
        }
    }

    #[tokio::test]
    async fn invalid_key_precedes_all_queries_and_read_failure_stops_replay() {
        let fixture = Fixture::new();
        let mut store = RecordingStore::new(vec![fixture.receipt()], Some(fixture.instance()));
        let error = replay_from_store(&store, fixture.expected(), " ", errors(), &mut NoTransaction)
            .await
            .unwrap_err();
        assert!(matches!(error, Error::ValidationError(_)));
        assert!(store.calls.lock().unwrap().is_empty());
        store.fail_at = Some("receipt");
        let error = replay_from_store(&store, fixture.expected(), "key-1", errors(), &mut NoTransaction)
            .await
            .unwrap_err();
        assert!(matches!(error, Error::ConflictError(message) if message == "original-read-error"));
        assert_eq!(store.calls.lock().unwrap().len(), 1);
    }
}
