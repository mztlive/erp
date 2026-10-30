use std::sync::atomic::{AtomicI64, AtomicUsize, Ordering};

use async_trait::async_trait;
use erp_core::AccountKind;
use erp_core::common::time::Instant;
use erp_integration::Result as IntegrationResult;
use erp_integration::entity::supplier_credential::SupplierCredentialPayload;
use tokio::sync::{Mutex, Notify};
use zeroize::Zeroizing;

use super::super::super::super::oauth::JdVopRefreshResult;
use super::super::http::AuthorizationGrant;
use super::*;

struct TestClock(AtomicI64);
impl Clock for TestClock {
    fn now(&self) -> Instant {
        Instant::from_unix_secs(self.0.load(Ordering::SeqCst))
    }
}

#[derive(Clone, Copy)]
enum Fail {
    None,
    Sent,
    Exchange,
    Save,
    CommittedUnknown,
}
struct MemoryState {
    record: Option<SupplierCredential>,
    writes: usize,
    failure: Fail,
    sent_delay: i64,
    exchange_delay: i64,
}
struct MemoryStore {
    state: Mutex<MemoryState>,
    clock: Arc<TestClock>,
}
impl MemoryStore {
    async fn change(
        &self,
        original: SupplierCredential,
        change: impl FnOnce(&mut SupplierCredential) -> IntegrationResult<()> + Send,
    ) -> Result<Option<SupplierCredential>> {
        let mut state = self.state.lock().await;
        if state.record.as_ref().is_none_or(|current| current.base.version != original.base.version) {
            return Ok(None);
        }
        let mut next = original;
        change(&mut next).map_err(|_| AuthorizationError::Conflict)?;
        state.record = Some(next.clone());
        state.writes += 1;
        Ok(Some(next))
    }
}
#[async_trait]
impl AuthorizationStore for MemoryStore {
    async fn load(&self) -> Result<Option<SupplierCredential>> {
        Ok(self.state.lock().await.record.clone())
    }
    async fn reserve(
        &self,
        request: SupplierCredentialAuthorizationRequest,
        _context: CommandContext,
    ) -> Result<(SupplierCredential, bool)> {
        let mut state = self.state.lock().await;
        let (next, changed) = match state.record.clone() {
            Some(mut current) => {
                let changed =
                    current.claim_authorization(&request).map_err(|_| AuthorizationError::Conflict)?;
                (current, changed)
            },
            None => (
                SupplierCredential::reserve_authorization(
                    "credential".into(),
                    CredentialCrypto::new(&settings()).unwrap().binding,
                    &request,
                )
                .map_err(|_| AuthorizationError::Conflict)?,
                true,
            ),
        };
        if changed {
            state.record = Some(next.clone());
            state.writes += 1;
        }
        Ok((next, changed))
    }
    async fn sent(
        &self,
        record: SupplierCredential,
        now: Instant,
        _context: CommandContext,
    ) -> Result<Option<SupplierCredential>> {
        let (failure, delay) = {
            let state = self.state.lock().await;
            (state.failure, state.sent_delay)
        };
        if matches!(failure, Fail::Sent) {
            return Err(AuthorizationError::Forbidden);
        }
        let next = self.change(record, |record| record.mark_authorization_sent(now)).await?;
        self.clock.0.fetch_add(delay, Ordering::SeqCst);
        Ok(next)
    }
    async fn exchange(
        &self,
        record: SupplierCredential,
        now: Instant,
        _context: CommandContext,
    ) -> Result<Option<SupplierCredential>> {
        let (failure, delay) = {
            let state = self.state.lock().await;
            (state.failure, state.exchange_delay)
        };
        if matches!(failure, Fail::Exchange) {
            return Err(AuthorizationError::Forbidden);
        }
        let next = self.change(record, |record| record.mark_authorization_exchange_sent(now)).await?;
        self.clock.0.fetch_add(delay, Ordering::SeqCst);
        Ok(next)
    }
    async fn reject(
        &self,
        record: SupplierCredential,
        now: Instant,
        _context: CommandContext,
    ) -> Result<Option<SupplierCredential>> {
        self.change(record, |record| record.reject_authorization(now)).await
    }
    async fn complete(
        &self,
        record: SupplierCredential,
        payload: SupplierCredentialPayload,
        uid: Option<String>,
        now: Instant,
        _context: CommandContext,
    ) -> Result<Option<SupplierCredential>> {
        let failure = self.state.lock().await.failure;
        if matches!(failure, Fail::Save) {
            return Err(AuthorizationError::Unknown);
        }
        let next = self.change(record, |record| record.complete_authorization(payload, uid, now)).await?;
        if matches!(failure, Fail::CommittedUnknown) {
            return Err(AuthorizationError::Unknown);
        }
        Ok(next)
    }
}

#[derive(Clone, Copy)]
enum Login {
    Code,
    Rejected,
    Unknown,
}
struct RemoteState {
    login: Login,
    exchange_unknown: bool,
    uid: String,
    gated: bool,
}
struct FakeHttp {
    state: Mutex<RemoteState>,
    codes: AtomicUsize,
    exchanges: AtomicUsize,
    started: Notify,
    release: Notify,
}
#[async_trait]
impl AuthorizationHttp for FakeHttp {
    async fn code(&self, _parameters: AuthorizationParameters) -> Result<Zeroizing<String>> {
        self.codes.fetch_add(1, Ordering::SeqCst);
        let (login, gated) = {
            let state = self.state.lock().await;
            (state.login, state.gated)
        };
        self.started.notify_one();
        if gated {
            self.release.notified().await;
        }
        match login {
            Login::Code => Ok(Zeroizing::new("only-code".into())),
            Login::Rejected => Err(AuthorizationError::Rejected),
            Login::Unknown => Err(AuthorizationError::Unknown),
        }
    }
    async fn exchange(&self, _parameters: Zeroizing<String>) -> Result<AuthorizationGrant> {
        self.exchanges.fetch_add(1, Ordering::SeqCst);
        let state = self.state.lock().await;
        if state.exchange_unknown {
            return Err(AuthorizationError::Unknown);
        }
        Ok(AuthorizationGrant {
            tokens: JdVopRefreshResult::parse(
                br#"{"code":0,"access_token":"new-access","refresh_token":"new-refresh","expires_in":86400}"#,
            )
            .unwrap(),
            identity: Some(Zeroizing::new(state.uid.clone())),
        })
    }
}

type Fixture = (AuthorizationEngine, Arc<MemoryStore>, Arc<FakeHttp>, Arc<TestClock>);
fn settings() -> JdVopConfig {
    serde_json::from_value(
        serde_json::json!({"enabled":true,"app_key":"app","app_secret":"secret","credential_id":"credential",
        "credential_encryption_key":"unit-only-independent-credential-key-32","account_scope":"account"}),
    )
    .unwrap()
}
fn fixture() -> Fixture {
    let clock = Arc::new(TestClock(AtomicI64::new(1000)));
    let store = Arc::new(MemoryStore {
        clock: clock.clone(),
        state: Mutex::new(MemoryState {
            record: None,
            writes: 0,
            failure: Fail::None,
            sent_delay: 0,
            exchange_delay: 0,
        }),
    });
    let http = Arc::new(FakeHttp {
        state: Mutex::new(RemoteState {
            login: Login::Code,
            exchange_unknown: false,
            uid: "uid:original".into(),
            gated: false,
        }),
        codes: AtomicUsize::new(0),
        exchanges: AtomicUsize::new(0),
        started: Notify::new(),
        release: Notify::new(),
    });
    let engine = AuthorizationEngine {
        crypto: CredentialCrypto::new(&settings()).unwrap(),
        settings: settings(),
        store: store.clone(),
        http: http.clone(),
        clock: clock.clone(),
        tasks: Arc::new(JdVopAuthorizationTasks::default()),
        owner_id: "owner".into(),
    };
    (engine, store, http, clock)
}
fn actor() -> AuditActor {
    AuditActor::new("actor".into(), "actor".into(), AccountKind::Admin)
}
fn request(operation: u32, username: &str, expected_version: Option<u64>) -> JdVopAuthorizationRequest {
    let vector: serde_json::Value =
        serde_json::from_str(include_str!("../protocol/rsa_offline_fixture.json")).unwrap();
    JdVopAuthorizationRequest {
        operation_id: format!("11111111-1111-4111-8111-{operation:012}"),
        expected_version,
        username: Zeroizing::new(username.into()),
        password: Zeroizing::new("offline-test-password".into()),
        redirect_uri: "https://erp.example/callback".into(),
        private_key: Zeroizing::new(vector["private_pem"].as_str().unwrap().into()),
        evidence_reference: None,
    }
}
async fn authorize(
    engine: &AuthorizationEngine,
    operation: u32,
    username: &str,
    expected: Option<u64>,
) -> JdVopAuthorizationStatus {
    let status =
        engine.authorize("connection", request(operation, username, expected), &actor()).await.unwrap();
    engine.tasks.idle().await.unwrap();
    status
}

#[tokio::test]
async fn first_authorization_returns_pending_then_saves_identity_and_only_local_readiness() {
    let (engine, store, http, _) = fixture();
    assert_eq!(engine.status(None).await.state, "missing");
    let initial = authorize(&engine, 1, "buyer", None).await;
    assert_eq!(initial.state, "authorizing");
    let status = engine.status(None).await;
    assert_eq!(status.state, "authorized");
    assert_eq!(status.operation_state, Some("completed"));
    assert!(status.message.contains("连接检查"));
    let record = store.load().await.unwrap().unwrap();
    assert!(record.account_identity_fingerprint.is_some() && record.authorized_uid_fingerprint.is_some());
    let output = serde_json::to_string(&status).unwrap();
    for secret in ["buyer", "offline-test-password", "new-access", "new-refresh", "PRIVATE KEY"] {
        assert!(!output.contains(secret));
    }
    assert_eq!(http.codes.load(Ordering::SeqCst), 1);
    assert_eq!(http.exchanges.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn explicit_first_rejection_allows_corrected_username_and_old_operation_replays_original_receipt() {
    let (engine, store, http, _) = fixture();
    http.state.lock().await.login = Login::Rejected;
    authorize(&engine, 1, "wrong-buyer", None).await;
    let rejected = engine.status(None).await;
    assert_eq!(rejected.state, "rejected");
    http.state.lock().await.login = Login::Code;
    authorize(&engine, 2, "correct-buyer", rejected.version).await;
    let writes = store.state.lock().await.writes;
    let replay = engine.authorize("connection", request(1, "wrong-buyer", None), &actor()).await.unwrap();
    assert_eq!(replay.state, "authorized");
    assert_eq!(replay.operation_id.as_deref(), Some(request(1, "wrong-buyer", None).operation_id.as_str()));
    assert_eq!(replay.operation_state, Some("rejected"));
    let queried = engine.status(Some(&request(1, "wrong-buyer", None).operation_id)).await;
    assert_eq!(queried.operation_state, Some("rejected"));
    assert_eq!(store.state.lock().await.writes, writes);
    assert_eq!(http.codes.load(Ordering::SeqCst), 2);
    assert_eq!(http.exchanges.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn same_operation_and_cross_account_requests_never_send_again() {
    let (engine, store, http, _) = fixture();
    authorize(&engine, 1, "buyer", None).await;
    let version = engine.status(None).await.version;
    let writes = store.state.lock().await.writes;
    assert_eq!(
        engine.authorize("connection", request(1, "buyer", None), &actor()).await.unwrap().operation_state,
        Some("completed")
    );
    assert_eq!(
        engine.authorize("connection", request(2, "different", version), &actor()).await.err(),
        Some(AuthorizationError::IdentityMismatch)
    );
    assert_eq!(store.state.lock().await.writes, writes);
    assert_eq!(http.codes.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn unknown_keeps_candidate_account_and_requires_new_operation_version_evidence_after_deadline() {
    let (engine, store, http, clock) = fixture();
    http.state.lock().await.login = Login::Unknown;
    authorize(&engine, 1, "buyer", None).await;
    let record = store.load().await.unwrap().unwrap();
    assert!(
        engine
            .authorize("connection", request(2, "other", Some(record.base.version)), &actor())
            .await
            .is_err()
    );
    let mut retry = request(2, "buyer", Some(record.base.version));
    retry.evidence_reference = Some("new-authorization".into());
    assert!(engine.authorize("connection", retry, &actor()).await.is_err());
    clock.0.store(record.refresh.deadline().unwrap().unix_secs() + 1, Ordering::SeqCst);
    assert_eq!(engine.status(None).await.state, "unknown");
    let mut recovery = request(2, "buyer", Some(record.base.version));
    recovery.evidence_reference = Some("new-authorization".into());
    http.state.lock().await.login = Login::Code;
    engine.authorize("connection", recovery, &actor()).await.unwrap();
    engine.tasks.idle().await.unwrap();
    assert_eq!(engine.status(None).await.state, "authorized");
    assert_eq!(
        engine.status(Some(&request(1, "buyer", None).operation_id)).await.operation_state,
        Some("superseded")
    );
    assert_eq!(http.codes.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn stale_login_response_cannot_exchange_after_new_authorization_epoch() {
    let (engine, store, http, clock) = fixture();
    http.state.lock().await.gated = true;
    engine.authorize("connection", request(1, "buyer", None), &actor()).await.unwrap();
    http.started.notified().await;
    let first = store.load().await.unwrap().unwrap();
    clock.0.store(first.refresh.deadline().unwrap().unix_secs() + 1, Ordering::SeqCst);
    let second = AuthorizationEngine {
        tasks: Arc::new(JdVopAuthorizationTasks::default()),
        owner_id: "second-owner".into(),
        ..engine.clone()
    };
    http.state.lock().await.gated = false;
    let mut recovery = request(2, "buyer", Some(first.base.version));
    recovery.evidence_reference = Some("new-authorization".into());
    second.authorize("connection", recovery, &actor()).await.unwrap();
    second.tasks.idle().await.unwrap();
    http.release.notify_one();
    engine.tasks.idle().await.unwrap();
    assert_eq!(http.codes.load(Ordering::SeqCst), 2);
    assert_eq!(http.exchanges.load(Ordering::SeqCst), 1);
    assert_eq!(
        engine.status(None).await.operation_id.as_deref(),
        Some(request(2, "buyer", None).operation_id.as_str())
    );
}

#[tokio::test]
async fn each_persisted_send_boundary_and_full_deadline_controls_remote_calls() {
    for (failure, sent_delay, exchange_delay, codes, exchanges) in [
        (Fail::Sent, 0, 0, 0, 0),
        (Fail::Exchange, 0, 0, 1, 0),
        (Fail::None, 30, 0, 0, 0),
        (Fail::None, 0, 30, 1, 0),
    ] {
        let (engine, store, http, _) = fixture();
        {
            let mut state = store.state.lock().await;
            state.failure = failure;
            state.sent_delay = sent_delay;
            state.exchange_delay = exchange_delay;
        }
        authorize(&engine, 1, "buyer", None).await;
        assert_eq!(http.codes.load(Ordering::SeqCst), codes);
        assert_eq!(http.exchanges.load(Ordering::SeqCst), exchanges);
    }
}

#[tokio::test]
async fn failed_save_and_uid_mismatch_keep_unknown_but_committed_unknown_is_readable() {
    for failure in [Fail::Save, Fail::CommittedUnknown] {
        let (engine, store, http, clock) = fixture();
        store.state.lock().await.failure = failure;
        authorize(&engine, 1, "buyer", None).await;
        if matches!(failure, Fail::Save) {
            clock.0.store(1100, Ordering::SeqCst);
            assert_eq!(engine.status(None).await.state, "unknown");
        } else {
            assert_eq!(engine.status(None).await.state, "authorized");
        }
        assert_eq!(http.exchanges.load(Ordering::SeqCst), 1);
    }
    let (engine, _, http, clock) = fixture();
    authorize(&engine, 1, "buyer", None).await;
    http.state.lock().await.uid = "uid:different".into();
    authorize(&engine, 2, "buyer", engine.status(None).await.version).await;
    clock.0.store(1100, Ordering::SeqCst);
    assert_eq!(engine.status(None).await.state, "unknown");
    assert_eq!(http.exchanges.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn detached_authorization_survives_response_drop_and_shutdown_waits_without_new_work() {
    let (engine, _, http, _) = fixture();
    http.state.lock().await.gated = true;
    let response = engine.authorize("connection", request(1, "buyer", None), &actor()).await.unwrap();
    drop(response);
    http.started.notified().await;
    let tasks = engine.tasks.clone();
    let shutdown = tokio::spawn(async move { tasks.shutdown().await });
    tokio::task::yield_now().await;
    assert!(!shutdown.is_finished());
    http.release.notify_one();
    shutdown.await.unwrap().unwrap();
    assert_eq!(engine.status(None).await.state, "authorized");
    assert!(
        engine
            .authorize("connection", request(2, "buyer", engine.status(None).await.version), &actor())
            .await
            .is_err()
    );
    assert_eq!(http.codes.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn absent_history_is_not_reported_as_a_known_terminal_operation() {
    let (engine, _, _, _) = fixture();
    authorize(&engine, 1, "buyer", None).await;
    let status = engine.status(Some(&request(99, "buyer", None).operation_id)).await;
    assert_eq!(status.state, "authorized");
    assert_eq!(status.operation_id, None);
    assert_eq!(status.operation_state, None);
}
