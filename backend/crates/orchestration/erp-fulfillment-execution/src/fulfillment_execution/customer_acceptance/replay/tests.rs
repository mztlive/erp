use std::collections::BTreeMap;
use std::sync::Mutex;

use erp_core::AccountKind;
use erp_core::common::time::Instant;
use erp_core::ids::{CustomerAcceptanceId, SalesOrderId};
use erp_fulfillment::entity::fulfillment::{AcceptanceResult, CustomerAcceptanceData};
use erp_identity::Error as IdentityError;
use erp_identity::testing::unbound_authorization;
use mongodb::error::Error as MongoError;
use persistence_core::testing::SnapshotExecutor;
use persistence_core::{Error as PersistenceError, ReadExecutor};
use serde_json::json;

use super::super::commit::commit_command_receipt;
use super::*;

struct Reader {
    result: Option<String>,
    rows: BTreeMap<String, CustomerAcceptance>,
    denied: Option<&'static str>,
    events: Mutex<Vec<(String, usize)>>,
}
impl Reader {
    fn record(&self, step: String, executor: &mut dyn Executor) -> Result<()> {
        assert!(executor.boundary().unwrap().is_snapshot());
        self.events.lock().unwrap().push((step.clone(), executor as *mut dyn Executor as *mut () as usize));
        if self.denied.is_some_and(|denied| step.starts_with(denied)) {
            return Err(Error::Forbidden(step));
        }
        Ok(())
    }
    fn steps(&self) -> Vec<String> {
        self.events.lock().unwrap().iter().map(|(step, _)| step.clone()).collect()
    }
}
#[async_trait]
impl AcceptanceReplay for Reader {
    async fn operation(
        &self,
        _: &AuditActor,
        permission: &Permission,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        self.record(format!("operation:{permission}"), executor)
    }
    async fn receipt(&self, _: &CommandReceipt, executor: &mut dyn Executor) -> Result<Option<String>> {
        self.record("receipt".into(), executor)?;
        Ok(self.result.clone())
    }
    async fn acceptance(&self, id: &str, executor: &mut dyn Executor) -> Result<CustomerAcceptance> {
        self.record(format!("acceptance:{id}"), executor)?;
        self.rows.get(id).cloned().ok_or_else(|| Error::NotFound(id.into()))
    }
    async fn sales(&self, _: &AuditActor, id: &str, executor: &mut dyn Executor) -> Result<()> {
        self.record(format!("sales:{id}"), executor)
    }
    async fn task(
        &self,
        _: &AuditActor,
        sales: &str,
        task: Option<&str>,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        assert_eq!(sales, "sales");
        assert_eq!(task, Some("task"));
        self.record("current-task:task".into(), executor)
    }
}
fn actor() -> AuditActor {
    AuditActor::new("actor".into(), "actor".into(), AccountKind::Admin)
}
fn request() -> CommitCustomerAcceptanceRequest {
    serde_json::from_value(json!({
        "sales_order_id":"sales", "expected_sales_order_version":1,
        "accepted_at":1700000000, "result":"PASSED", "lines":[],
        "idempotency_key":"key", "work_item_id":"task", "expected_task_version":1,
    }))
    .unwrap()
}
fn receipt() -> CommandReceipt {
    commit_command_receipt(&request(), "actor").unwrap()
}
fn acceptance(id: &str) -> CustomerAcceptance {
    let mut acceptance = CustomerAcceptance::new(
        CustomerAcceptanceId::new(id),
        CustomerAcceptanceData {
            acceptance_no: format!("CA-{id}"),
            sales_order_id: SalesOrderId::new("sales"),
            accepted_at: Instant::from_unix_secs(1700000000),
            result: AcceptanceResult::Passed,
            evidence_attachment_id: None,
        },
    )
    .unwrap();
    acceptance.status = CustomerAcceptanceState::Posted;
    acceptance
}
fn reader() -> Reader {
    let result = acceptance("result");
    Reader {
        result: Some("result".into()),
        rows: [("result".into(), result), ("original".into(), acceptance("original"))].into(),
        denied: None,
        events: Mutex::new(vec![]),
    }
}

fn reverse_reader() -> Reader {
    let mut reader = reader();
    reader.rows.get_mut("result").unwrap().reversal_of_acceptance_id =
        Some(CustomerAcceptanceId::new("original"));
    reader
}

#[tokio::test]
async fn commit_replay_uses_current_task_and_one_snapshot_without_replaying_writes() {
    let reader = reader();
    let mut executor = SnapshotExecutor::default();
    let pointer = &mut executor as *mut SnapshotExecutor as usize;
    let result = committed(&reader, &request(), &actor(), &receipt(), &mut executor).await.unwrap().unwrap();
    assert_eq!(result.base.id, "result");
    assert_eq!(
        reader.steps(),
        [
            "operation:customer_acceptance:post",
            "receipt",
            "acceptance:result",
            "sales:sales",
            "current-task:task"
        ]
    );
    assert!(reader.events.lock().unwrap().iter().all(|(_, seen)| *seen == pointer));
}

#[tokio::test]
async fn commit_replay_stops_on_revoked_operation_source_or_task() {
    for denied in ["operation", "sales", "current-task"] {
        let reader = Reader { denied: Some(denied), ..reader() };
        let error = committed(&reader, &request(), &actor(), &receipt(), &mut SnapshotExecutor::default())
            .await
            .unwrap_err();
        assert!(matches!(error, Error::Forbidden(_)));
        assert!(reader.steps().last().unwrap().starts_with(denied));
    }
    let reader = Reader { result: None, ..reader() };
    assert!(
        committed(&reader, &request(), &actor(), &receipt(), &mut SnapshotExecutor::default())
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(reader.steps(), ["operation:customer_acceptance:post", "receipt"]);
}

#[tokio::test]
async fn commit_replay_rejects_changed_result_sales_or_unposted_receipt_result() {
    for change in ["sales", "status", "draft", "reversal"] {
        let mut reader = reader();
        let mut req = request();
        let result = reader.rows.get_mut("result").unwrap();
        match change {
            "sales" => result.sales_order_id = SalesOrderId::new("another"),
            "status" => result.status = CustomerAcceptanceState::Draft,
            "draft" => req.acceptance_id = Some("another-draft".into()),
            _ => result.reversal_of_acceptance_id = Some(CustomerAcceptanceId::new("another")),
        }
        let error = committed(&reader, &req, &actor(), &receipt(), &mut SnapshotExecutor::default())
            .await
            .unwrap_err();
        assert!(matches!(error, Error::ConflictError(_)));
        assert_eq!(reader.steps(), ["operation:customer_acceptance:post", "receipt", "acceptance:result"]);
    }
    let mut req = request();
    req.acceptance_id = Some("result".into());
    assert!(
        committed(&reader(), &req, &actor(), &receipt(), &mut SnapshotExecutor::default())
            .await
            .unwrap()
            .is_some()
    );
}

#[tokio::test]
async fn reverse_replay_requires_operation_and_real_original_source_in_the_same_snapshot() {
    let recording = reverse_reader();
    let reader = &recording;
    let mut executor = SnapshotExecutor::default();
    let pointer = &mut executor as *mut SnapshotExecutor as usize;
    let result = reversed(reader, "original", &actor(), &receipt(), &mut executor).await.unwrap().unwrap();
    assert_eq!(result.reversal_of_acceptance_id.unwrap().as_ref(), "original");
    assert_eq!(
        reader.steps(),
        [
            "operation:customer_acceptance:reverse",
            "acceptance:original",
            "sales:sales",
            "receipt",
            "acceptance:result"
        ]
    );
    assert!(reader.events.lock().unwrap().iter().all(|(_, seen)| *seen == pointer));
    let reader = Reader { denied: Some("operation"), ..self::reader() };
    assert!(matches!(
        reversed(&reader, "original", &actor(), &receipt(), &mut SnapshotExecutor::default()).await,
        Err(Error::Forbidden(_))
    ));
    assert_eq!(reader.steps(), ["operation:customer_acceptance:reverse"]);
}

#[tokio::test]
async fn reverse_replay_refuses_a_different_original_sales_or_draft_result() {
    for change in ["original", "sales", "status"] {
        let mut reader = reverse_reader();
        let result = reader.rows.get_mut("result").unwrap();
        match change {
            "original" => result.reversal_of_acceptance_id = Some(CustomerAcceptanceId::new("another")),
            "sales" => result.sales_order_id = SalesOrderId::new("another"),
            _ => result.status = CustomerAcceptanceState::Draft,
        }
        assert!(matches!(
            reversed(&reader, "original", &actor(), &receipt(), &mut SnapshotExecutor::default()).await,
            Err(Error::ConflictError(_))
        ));
    }
}

#[tokio::test]
async fn both_replay_commands_refuse_non_snapshot_before_reading() {
    let reader = reader();
    assert!(matches!(
        committed(&reader, &request(), &actor(), &receipt(), &mut ReadExecutor::default()).await,
        Err(Error::ValidationError(_))
    ));
    assert!(matches!(
        reversed(&reader, "original", &actor(), &receipt(), &mut ReadExecutor::default()).await,
        Err(Error::ValidationError(_))
    ));
    assert!(reader.steps().is_empty());
}

#[tokio::test]
async fn actual_commit_alias_and_reverse_operation_targets_reach_identity_qualification() {
    let db = mongodb::Client::with_uri_str("mongodb://127.0.0.1:1/?serverSelectionTimeoutMS=1")
        .await
        .unwrap()
        .database("acceptance_operation_shape");
    let reader = ReplayReader { db, authorization: unbound_authorization() };
    for permission in [
        fulfillment::CUSTOMER_ACCEPTANCE_CUSTOMER_ACCEPTANCE_COMMIT,
        fulfillment::CUSTOMER_ACCEPTANCE_CUSTOMER_ACCEPTANCE_REVERSE,
    ] {
        let error = reader
            .authorization
            .require(&actor(), &permission, AccessTarget::Operation, &mut SnapshotExecutor::default())
            .await
            .unwrap_err();
        assert!(matches!(error, IdentityError::Internal(message) if message == "测试授权设施不可用"));
    }
}

#[tokio::test]
async fn known_command_failures_never_start_recovery_reads() {
    for original in [Error::Forbidden("denied".into()), Error::ConflictError("version".into())] {
        let visited = Mutex::new(false);
        let lookup = async {
            *visited.lock().unwrap() = true;
            Ok(Some("must-not-recover"))
        };
        assert!(recover(original, lookup).await.is_err());
        assert!(!*visited.lock().unwrap());
    }
    for lookup in [Ok(None), Err(Error::Forbidden("revoked".into()))] {
        let result = recover::<&str>(
            Error::OutcomeUnknown(PersistenceError::CommitOutcomeUnknown(MongoError::custom(
                "original-commit",
            ))),
            async { lookup },
        )
        .await;
        assert!(
            matches!(result, Err(Error::OutcomeUnknown(PersistenceError::CommitOutcomeUnknown(source))) if source.get_custom::<&'static str>() == Some(&"original-commit"))
        );
    }
    assert_eq!(
        recover(
            Error::OutcomeUnknown(PersistenceError::CommitOutcomeUnknown(MongoError::custom(
                "original-commit"
            ))),
            async { Ok(Some("persisted-result")) }
        )
        .await
        .unwrap(),
        "persisted-result"
    );
}
