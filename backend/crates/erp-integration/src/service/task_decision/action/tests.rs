//! 实际动作事实函数使用注入权威端口的顺序与失败停止证据。
use std::sync::Mutex;

use super::*;
use crate::ports::evidence::{EvidenceFuture, VerifiedEvidence};

struct TestExecutor {
    _identity: u8,
}

impl Executor for TestExecutor {
    fn session(&mut self) -> Option<&mut mongodb::ClientSession> {
        None
    }
}

struct Authority {
    original: OriginalResultFact,
    fail: Option<&'static str>,
    seen: Mutex<Vec<(&'static str, usize)>>,
}

impl Authority {
    fn record(&self, step: &'static str, executor: &mut dyn Executor) -> Result<()> {
        self.seen.lock().unwrap().push((step, executor as *mut dyn Executor as *mut () as usize));
        if self.fail == Some(step) {
            return Err(Error::ConflictError(format!("authority:{step}")));
        }
        Ok(())
    }
}

impl IntegrationEvidenceAuthority for Authority {
    fn query_original<'a>(
        &'a self,
        _: &'a EvidenceSubject,
        executor: &'a mut dyn Executor,
    ) -> EvidenceFuture<'a, OriginalResultFact> {
        Box::pin(async move {
            self.record("query", executor)?;
            Ok(self.original.clone())
        })
    }
    fn replay_original<'a>(
        &'a self,
        _: &'a EvidenceSubject,
        executor: &'a mut dyn Executor,
    ) -> EvidenceFuture<'a, String> {
        Box::pin(async move {
            self.record("replay", executor)?;
            Ok("replay-reference".to_string())
        })
    }
    fn verify_reattribution<'a>(
        &'a self,
        _: &'a EvidenceSubject,
        executor: &'a mut dyn Executor,
    ) -> EvidenceFuture<'a, String> {
        Box::pin(async move {
            self.record("reattribute", executor)?;
            Ok("reattribution-reference".to_string())
        })
    }
    fn verify_evidence<'a>(
        &'a self,
        _: &'a EvidenceSubject,
        _: &'a ControlledEvidenceRef,
        _: &'a str,
        executor: &'a mut dyn Executor,
    ) -> EvidenceFuture<'a, VerifiedEvidence> {
        Box::pin(async move {
            self.record("verify", executor)?;
            Err(Error::Internal("unexpected verify".to_string()))
        })
    }
    fn discover_evidence<'a>(
        &'a self,
        _: &'a EvidenceSubject,
        executor: &'a mut dyn Executor,
    ) -> EvidenceFuture<'a, Vec<ControlledEvidenceRef>> {
        Box::pin(async move {
            self.record("discover", executor)?;
            Ok(Vec::new())
        })
    }
}

fn subject() -> EvidenceSubject {
    EvidenceSubject {
        item_id: "task-1".to_string(),
        message_id: Some("message-1".to_string()),
        business_object_type: None,
        business_object_id: None,
        fact_references: Vec::new(),
    }
}

#[tokio::test]
async fn terminal_query_discovers_after_query_on_same_injected_executor() {
    let authority = Authority {
        original: OriginalResultFact::Terminal("inbox_message:message-1".to_string()),
        fail: None,
        seen: Mutex::new(Vec::new()),
    };
    let mut executor = TestExecutor { _identity: 1 };
    let identity = &mut executor as *mut TestExecutor as usize;
    let fact = query_action_fact(&authority, &subject(), &mut executor).await.unwrap();
    assert_eq!(fact.outcome, IntegrationActionOutcome::TerminalEvidenceFound);
    assert_eq!(fact.business_result_reference.as_deref(), Some("inbox_message:message-1"));
    assert_eq!(*authority.seen.lock().unwrap(), [("query", identity), ("discover", identity)]);
}

#[tokio::test]
async fn authority_error_is_preserved_and_no_later_call_runs() {
    for step in ["query", "discover"] {
        let authority = Authority {
            original: OriginalResultFact::Terminal("inbox_message:message-1".to_string()),
            fail: Some(step),
            seen: Mutex::new(Vec::new()),
        };
        let mut executor = TestExecutor { _identity: 2 };
        let identity = &mut executor as *mut TestExecutor as usize;
        let result = query_action_fact(&authority, &subject(), &mut executor).await;
        assert!(
            matches!(result,Err(Error::ConflictError(message)) if message == format!("authority:{step}"))
        );
        let expected = if step == "query" {
            vec![("query", identity)]
        } else {
            vec![("query", identity), ("discover", identity)]
        };
        assert_eq!(*authority.seen.lock().unwrap(), expected);
    }
}

#[tokio::test]
async fn absent_or_unknown_original_does_not_discover_or_replay() {
    for (original, outcome) in [
        (OriginalResultFact::NoResult, IntegrationActionOutcome::NoResultConfirmed),
        (OriginalResultFact::Unknown, IntegrationActionOutcome::ResultUnknown),
    ] {
        let authority = Authority { original, fail: None, seen: Mutex::new(Vec::new()) };
        let mut executor = TestExecutor { _identity: 3 };
        let identity = &mut executor as *mut TestExecutor as usize;
        let fact = query_action_fact(&authority, &subject(), &mut executor).await.unwrap();
        assert_eq!(fact.outcome, outcome);
        assert!(fact.business_result_reference.is_none());
        assert!(fact.verified_evidence.is_empty());
        assert_eq!(*authority.seen.lock().unwrap(), [("query", identity)]);
    }
}
