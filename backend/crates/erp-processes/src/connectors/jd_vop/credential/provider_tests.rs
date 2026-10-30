use std::sync::atomic::{AtomicI64, AtomicUsize, Ordering};

use application_core::AuditActor;
use erp_core::AccountKind;
use erp_core::common::time::Instant;
use erp_integration::entity::supplier_credential::{
    SupplierCredentialAuthorizationRequest, SupplierCredentialPayload, SupplierCredentialReauthorization,
};
use tokio::sync::{Mutex, Notify};

use super::super::{JdVopCredentialImport, JdVopCredentialProcess, JdVopCredentialStatus, validate_import};
use super::*;

struct TestClock(AtomicI64);
impl Clock for TestClock {
    fn now(&self) -> Instant {
        Instant::from_unix_secs(self.0.load(Ordering::SeqCst))
    }
}

#[derive(Clone, Copy)]
enum SaveMode {
    Normal,
    Fail,
    CommittedUnknown,
}

struct Stored {
    record: Option<SupplierCredential>,
    writes: usize,
    save_mode: SaveMode,
    reject_sent: bool,
    sent_delay_seconds: i64,
}
struct MemoryStore {
    state: Mutex<Stored>,
    clock: Arc<TestClock>,
}

#[async_trait]
impl CredentialStore for MemoryStore {
    async fn load(&self) -> ConnectorResult<Option<SupplierCredential>> {
        Ok(self.state.lock().await.record.clone())
    }
    async fn import(
        &self,
        payload: SupplierCredentialPayload,
        expected: Option<u64>,
        recovery: Option<SupplierCredentialReauthorization>,
        _actor: AuditActor,
        now: Instant,
    ) -> ConnectorResult<SupplierCredential> {
        let crypto = CredentialCrypto::new(&settings()).unwrap();
        let mut state = self.state.lock().await;
        validate_import(&crypto, &mut state.record, payload, expected, recovery.as_ref(), now)?;
        state.writes += 1;
        Ok(state.record.clone().unwrap())
    }
    async fn claim(
        &self,
        version: u64,
        claim: SupplierCredentialRefreshClaim,
    ) -> ConnectorResult<Option<SupplierCredential>> {
        let mut state = self.state.lock().await;
        let Some(record) = state.record.as_mut() else { return Ok(None) };
        if record.base.version != version {
            return Ok(None);
        }
        if !record.claim_refresh(version, &claim).map_err(|_| token_error())? {
            return Ok(None);
        }
        let record = record.clone();
        state.writes += 1;
        Ok(Some(record))
    }
    async fn sent(
        &self,
        claimed: SupplierCredential,
        now: Instant,
    ) -> ConnectorResult<Option<SupplierCredential>> {
        let mut state = self.state.lock().await;
        if state.reject_sent {
            return Err(token_error());
        }
        let Some(record) = state.record.as_mut() else { return Ok(None) };
        if record.base.version != claimed.base.version {
            return Ok(None);
        }
        record.mark_refresh_sent(now).map_err(|_| token_error())?;
        let record = record.clone();
        state.writes += 1;
        self.clock.0.fetch_add(state.sent_delay_seconds, Ordering::SeqCst);
        Ok(Some(record))
    }
    async fn complete(
        &self,
        sent: SupplierCredential,
        payload: SupplierCredentialPayload,
        now: Instant,
    ) -> ConnectorResult<Option<SupplierCredential>> {
        let mut state = self.state.lock().await;
        if matches!(state.save_mode, SaveMode::Fail) {
            return Err(token_error());
        }
        let Some(record) = state.record.as_mut() else { return Ok(None) };
        if record.base.version != sent.base.version {
            return Ok(None);
        }
        record.complete_refresh(payload, now).map_err(|_| token_error())?;
        let record = record.clone();
        state.writes += 1;
        if matches!(state.save_mode, SaveMode::CommittedUnknown) {
            return Err(token_error());
        }
        Ok(Some(record))
    }
    async fn invalidate(
        &self,
        sent: SupplierCredential,
        now: Instant,
    ) -> ConnectorResult<Option<SupplierCredential>> {
        let mut state = self.state.lock().await;
        let Some(record) = state.record.as_mut() else { return Ok(None) };
        if record.base.version != sent.base.version {
            return Ok(None);
        }
        record.invalidate_refresh(now).map_err(|_| token_error())?;
        let record = record.clone();
        state.writes += 1;
        Ok(Some(record))
    }
}

#[derive(Clone, Copy)]
enum Reply {
    Success,
    NetworkUnknown,
    InvalidGrant,
}
struct FakeOAuth {
    reply: Reply,
    calls: AtomicUsize,
    gated: bool,
    started: Notify,
    release: Notify,
}
#[async_trait]
impl OAuthRefresh for FakeOAuth {
    async fn refresh(&self, request: JdVopRefreshRequest) -> Result<JdVopRefreshResult, RefreshFailure> {
        assert_eq!(request.endpoint(), "https://open-oauth.jd.com/oauth2/refresh_token");
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.started.notify_one();
        if self.gated {
            self.release.notified().await;
        }
        match self.reply {
            Reply::Success => JdVopRefreshResult::parse(
                br#"{"code":0,"access_token":"new-access","refresh_token":"new-refresh","expires_in":86400}"#,
            )
            .map_err(|_| RefreshFailure::Unknown),
            Reply::NetworkUnknown => Err(RefreshFailure::Unknown),
            Reply::InvalidGrant => Err(RefreshFailure::AuthorizationInvalid),
        }
    }
}

fn settings() -> JdVopConfig {
    serde_json::from_value(
        serde_json::json!({"app_key":"app","app_secret":"app-secret", "account_scope":"account",
        "credential_id":"credential", "credential_encryption_key":"unit-only-independent-credential-key-32"}),
    )
    .unwrap()
}
fn input() -> JdVopCredentialImport {
    JdVopCredentialImport {
        credential_id: "credential".into(),
        account_scope: "account".into(),
        app_key: "app".into(),
        access_token: "old-access".into(),
        refresh_token: "old-refresh".into(),
        expires_at: 87_400,
    }
}
fn actor() -> AuditActor {
    AuditActor::new("operator".into(), "operator".into(), AccountKind::Admin)
}

fn fixture(
    reply: Reply,
    gated: bool,
) -> (Arc<MongoJdVopTokenProvider>, JdVopCredentialProcess, Arc<MemoryStore>, Arc<FakeOAuth>, Arc<TestClock>)
{
    let clock = Arc::new(TestClock(AtomicI64::new(1000)));
    let crypto = CredentialCrypto::new(&settings()).unwrap();
    let payload = crypto.seal(input(), clock.now()).unwrap();
    let record =
        SupplierCredential::new(crypto.id.clone(), crypto.binding.clone(), payload, clock.now()).unwrap();
    let store = Arc::new(MemoryStore {
        clock: clock.clone(),
        state: Mutex::new(Stored {
            record: Some(record),
            writes: 0,
            save_mode: SaveMode::Normal,
            reject_sent: false,
            sent_delay_seconds: 0,
        }),
    });
    let oauth = Arc::new(FakeOAuth {
        reply,
        calls: AtomicUsize::new(0),
        gated,
        started: Notify::new(),
        release: Notify::new(),
    });
    let provider = provider(store.clone(), oauth.clone(), clock.clone());
    let process = JdVopCredentialProcess {
        crypto: CredentialCrypto::new(&settings()).unwrap(),
        store: store.clone(),
        clock: clock.clone(),
    };
    (Arc::new(provider), process, store, oauth, clock)
}
fn provider(
    store: Arc<MemoryStore>,
    oauth: Arc<FakeOAuth>,
    clock: Arc<TestClock>,
) -> MongoJdVopTokenProvider {
    MongoJdVopTokenProvider {
        crypto: CredentialCrypto::new(&settings()).unwrap(),
        app_secret: "app-secret".into(),
        timeout_seconds: 15,
        store,
        oauth,
        clock,
        owner_id: next_id(),
        refresh_slot: Arc::new(Semaphore::new(1)),
    }
}
fn due(clock: &TestClock) {
    clock.0.store(29_801, Ordering::SeqCst);
}

#[tokio::test]
async fn read_before_refresh_window_is_read_only_then_due_refresh_rotates_once() {
    let (provider, process, store, oauth, clock) = fixture(Reply::Success, false);
    assert_eq!(
        provider.access_token("credential").await.unwrap().value_for("account", clock.now()).unwrap(),
        "old-access"
    );
    assert_eq!(store.state.lock().await.writes, 0);
    due(&clock);
    let token = provider.access_token("credential").await.unwrap();
    assert_eq!(token.value_for("account", clock.now()).unwrap(), "new-access");
    assert_eq!(token.version(), "4");
    assert_eq!(oauth.calls.load(Ordering::SeqCst), 1);
    assert_eq!(process.status().await.unwrap().refresh_stage, "READY");
    assert!(provider.access_token("credential").await.is_ok());
    assert_eq!(oauth.calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn independent_process_owners_share_one_refresh_and_sent_unknown_blocks_old_access() {
    let (first, _, store, oauth, clock) = fixture(Reply::Success, true);
    let second = provider(store.clone(), oauth.clone(), clock.clone());
    due(&clock);
    let task = tokio::spawn(async move { first.access_token("credential").await });
    oauth.started.notified().await;
    assert!(second.access_token("credential").await.is_err());
    assert_eq!(oauth.calls.load(Ordering::SeqCst), 1);
    oauth.release.notify_one();
    assert!(task.await.unwrap().is_ok());
    assert!(second.access_token("credential").await.is_ok());
    assert_eq!(oauth.calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn cancelling_business_waiter_does_not_cancel_refresh_save_and_shutdown_waits_for_it() {
    let (provider, process, _, oauth, clock) = fixture(Reply::Success, true);
    due(&clock);
    let caller = provider.clone();
    let waiter = tokio::spawn(async move { caller.access_token("credential").await });
    oauth.started.notified().await;
    waiter.abort();
    assert!(waiter.await.is_err());
    let stopping = provider.clone();
    let settle = tokio::spawn(async move { stopping.settle_refresh().await });
    tokio::task::yield_now().await;
    assert!(!settle.is_finished());
    assert_eq!(process.status().await.unwrap().refresh_stage, "SENT_UNKNOWN");
    oauth.release.notify_one();
    settle.await.unwrap().unwrap();
    assert_eq!(process.status().await.unwrap().refresh_stage, "READY");
    let token = provider.access_token("credential").await.unwrap();
    assert_eq!(token.value_for("account", clock.now()).unwrap(), "new-access");
    assert_eq!(oauth.calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn network_unknown_never_reuses_old_access_or_resends_even_after_lease_and_token_expiry() {
    let (provider, process, _, oauth, clock) = fixture(Reply::NetworkUnknown, false);
    due(&clock);
    assert!(provider.access_token("credential").await.is_err());
    assert_eq!(process.status().await.unwrap().refresh_stage, "SENT_UNKNOWN");
    for time in [30_000, 100_000] {
        clock.0.store(time, Ordering::SeqCst);
        assert!(provider.access_token("credential").await.is_err());
    }
    assert_eq!(oauth.calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn failed_save_remains_unknown_but_unknown_commit_recovers_only_persisted_new_token() {
    for mode in [SaveMode::Fail, SaveMode::CommittedUnknown] {
        let (provider, process, store, oauth, clock) = fixture(Reply::Success, false);
        store.state.lock().await.save_mode = mode;
        due(&clock);
        let result = provider.access_token("credential").await;
        if matches!(mode, SaveMode::Fail) {
            assert!(result.is_err());
            assert_eq!(process.status().await.unwrap().refresh_stage, "SENT_UNKNOWN");
        } else {
            assert_eq!(result.unwrap().value_for("account", clock.now()).unwrap(), "new-access");
            assert_eq!(process.status().await.unwrap().refresh_stage, "READY");
        }
        let _ = provider.access_token("credential").await;
        assert_eq!(oauth.calls.load(Ordering::SeqCst), 1);
    }
}

#[tokio::test]
async fn known_revocation_never_falls_back_or_refreshes_again() {
    let (provider, process, _, oauth, clock) = fixture(Reply::InvalidGrant, false);
    due(&clock);
    assert!(provider.access_token("credential").await.is_err());
    assert_eq!(process.status().await.unwrap().refresh_stage, "REAUTHORIZATION_REQUIRED");
    assert!(provider.access_token("credential").await.is_err());
    assert_eq!(oauth.calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn expired_authorization_is_zero_http_including_expired_unsent_claim() {
    for claimed in [false, true] {
        let (provider, process, store, oauth, clock) = fixture(Reply::Success, false);
        if claimed {
            let mut state = store.state.lock().await;
            state
                .record
                .as_mut()
                .unwrap()
                .claim_refresh(
                    1,
                    &SupplierCredentialRefreshClaim {
                        owner_id: "owner".into(),
                        operation_id: "unsent".into(),
                        now: clock.now(),
                        lease_seconds: 20,
                    },
                )
                .unwrap();
        }
        clock.0.store(input().expires_at, Ordering::SeqCst);
        assert!(provider.access_token("credential").await.is_err());
        assert!(process.status().await.unwrap().reauthorization_required);
        assert_eq!(oauth.calls.load(Ordering::SeqCst), 0);
    }
}

#[tokio::test]
async fn sent_persist_failure_is_zero_http_and_keeps_unsent_access_while_valid() {
    let (provider, _, store, oauth, clock) = fixture(Reply::Success, false);
    store.state.lock().await.reject_sent = true;
    due(&clock);
    assert_eq!(
        provider.access_token("credential").await.unwrap().value_for("account", clock.now()).unwrap(),
        "old-access"
    );
    assert_eq!(oauth.calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn insufficient_remaining_send_deadline_is_zero_http_and_unknown_is_not_reclaimed() {
    for delay in [5, 6] {
        let (provider, process, store, oauth, clock) = fixture(Reply::Success, false);
        store.state.lock().await.sent_delay_seconds = delay;
        due(&clock);
        assert!(provider.access_token("credential").await.is_err());
        assert_eq!(process.status().await.unwrap().refresh_stage, "SENT_UNKNOWN");
        assert_eq!(oauth.calls.load(Ordering::SeqCst), 0);
    }
}

#[tokio::test]
async fn import_preview_is_zero_write_and_apply_requires_original_binding_and_version() {
    let (_, process, store, oauth, clock) = fixture(Reply::Success, false);
    assert!(process.preview(&input(), Some(1), None).await.is_ok());
    assert!(process.preview(&input(), None, None).await.is_err());
    assert_eq!(store.state.lock().await.writes, 0);
    let mut wrong = input();
    wrong.account_scope = "other".into();
    assert!(process.import(wrong, Some(1), None, &actor()).await.is_err());
    assert_eq!(store.state.lock().await.writes, 0);
    let status = process.import(input(), Some(1), None, &actor()).await.unwrap();
    assert_eq!(status.version, 2);
    assert_eq!(oauth.calls.load(Ordering::SeqCst), 0);
    assert!(process.import(input(), Some(1), None, &actor()).await.is_err());
    let mut invalid = input();
    invalid.expires_at = i64::MAX;
    assert!(process.preview(&invalid, Some(2), None).await.is_err());
    clock.0.store(100_000, Ordering::SeqCst);
    assert!(process.preview(&input(), Some(2), None).await.is_err());
}

#[tokio::test]
async fn explicit_new_authorization_after_deadline_fences_old_refresh_response() {
    let (provider, process, store, oauth, clock) = fixture(Reply::Success, true);
    due(&clock);
    let running = provider.clone();
    let task = tokio::spawn(async move { running.access_token("credential").await });
    oauth.started.notified().await;
    let status = process.status().await.unwrap();
    assert!(process.preview(&input(), Some(status.version), None).await.is_err());
    assert!(process.preview(&input(), Some(status.version), Some("new-authorization")).await.is_err());
    clock.0.store(status.refresh_deadline.unwrap() + 1, Ordering::SeqCst);
    assert!(process.preview(&input(), Some(status.version), Some("invalid\nevidence")).await.is_err());
    let mut imported = input();
    imported.access_token = "reauthorized-access".into();
    let new_status = process
        .import(imported, Some(status.version), Some("new-authorization".into()), &actor())
        .await
        .unwrap();
    oauth.release.notify_one();
    let token = task.await.unwrap().unwrap();
    assert_eq!(token.value_for("account", clock.now()).unwrap(), "reauthorized-access");
    assert_eq!(process.status().await.unwrap().version, new_status.version);
    assert_eq!(store.state.lock().await.record.as_ref().unwrap().reauthorization_history.len(), 1);
    assert_eq!(oauth.calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn ciphertext_identity_and_expiry_substitution_fail_before_http_without_secret_output() {
    let (provider, process, store, oauth, clock) = fixture(Reply::Success, false);
    assert!(provider.access_token("other-reference").await.is_err());
    store.state.lock().await.record.as_mut().unwrap().expires_at = Instant::from_unix_secs(90_000);
    due(&clock);
    let error = provider.access_token("credential").await.err().unwrap();
    for secret in ["old-access", "old-refresh", "app-secret", "unit-only-independent"] {
        assert!(!error.summary.contains(secret));
    }
    let status: JdVopCredentialStatus = process.status().await.unwrap();
    let output = serde_json::to_string(&status).unwrap();
    assert_eq!(status.account_scope, "[REDACTED]");
    assert_eq!(status.app_key, "[REDACTED]");
    assert!(!output.contains("old-access") && !output.contains("old-refresh"));
    assert_eq!(oauth.calls.load(Ordering::SeqCst), 0);
}

#[test]
fn encryption_uses_random_nonce_and_stable_secret_hmac_for_same_authorization() {
    let crypto = CredentialCrypto::new(&settings()).unwrap();
    let first = crypto.seal(input(), Instant::from_unix_secs(1000)).unwrap();
    let second = crypto.seal(input(), Instant::from_unix_secs(1001)).unwrap();
    assert_ne!(first.encrypted_payload, second.encrypted_payload);
    assert_eq!(first.payload_fingerprint, second.payload_fingerprint);
    assert!(!first.encrypted_payload.contains("old-access"));
    let mut record = SupplierCredential::new(
        "credential".into(),
        crypto.binding.clone(),
        first,
        Instant::from_unix_secs(1000),
    )
    .unwrap();
    assert!(crypto.open(&record).is_ok());
    record.account_scope = "other".into();
    assert!(crypto.open(&record).is_err());
}

#[test]
fn semantic_reauthorization_replay_accepts_new_nonce_without_changing_version_or_history() {
    let crypto = CredentialCrypto::new(&settings()).unwrap();
    let now = Instant::from_unix_secs(1000);
    let original = crypto.seal(input(), now).unwrap();
    let mut record =
        SupplierCredential::new("credential".into(), crypto.binding.clone(), original, now).unwrap();
    record
        .claim_refresh(
            1,
            &SupplierCredentialRefreshClaim {
                owner_id: "owner".into(),
                operation_id: "refresh".into(),
                now,
                lease_seconds: 1,
            },
        )
        .unwrap();
    record.mark_refresh_sent(now).unwrap();
    let recovery = SupplierCredentialReauthorization {
        expected_version: record.base.version,
        new_authorization: true,
        operation_id: "recovery".into(),
        actor_id: "operator".into(),
        evidence_reference: "authorization".into(),
    };
    let after = Instant::from_unix_secs(1002);
    let first = crypto.seal(input(), after).unwrap();
    assert!(record.reauthorize(first, &recovery, after).unwrap());
    let version = record.base.version;
    let repeated = crypto.seal(input(), Instant::from_unix_secs(1003)).unwrap();
    assert!(!record.reauthorize(repeated, &recovery, Instant::from_unix_secs(1003)).unwrap());
    assert_eq!(record.base.version, version);
    assert_eq!(record.reauthorization_history.len(), 1);
}

#[tokio::test]
async fn page_authorization_claim_send_exchange_and_rejection_never_deliver_old_token_or_refresh() {
    for phase in [0, 1, 2, 3] {
        let (provider, _, store, oauth, clock) = fixture(Reply::Success, false);
        let identity = provider.crypto.identity_fingerprint("username", "buyer").unwrap();
        {
            let mut state = store.state.lock().await;
            let record = state.record.as_mut().unwrap();
            record.account_identity_fingerprint = Some(identity.clone());
            let request = SupplierCredentialAuthorizationRequest {
                expected_version: Some(record.base.version),
                claim: SupplierCredentialRefreshClaim {
                    owner_id: "page-owner".into(),
                    operation_id: "page-operation".into(),
                    now: clock.now(),
                    lease_seconds: 45,
                },
                new_authorization: true,
                actor_id: "operator".into(),
                evidence_reference: "interactive".into(),
                account_identity_fingerprint: identity,
            };
            assert!(record.claim_authorization(&request).unwrap());
            if phase > 0 {
                record.mark_authorization_sent(clock.now()).unwrap();
            }
            if phase == 2 {
                record.mark_authorization_exchange_sent(clock.now()).unwrap();
            }
            if phase == 3 {
                record.reject_authorization(clock.now()).unwrap();
            }
        }
        assert!(provider.access_token("credential").await.is_err());
        assert!(provider.current().await.is_err());
        assert_eq!(oauth.calls.load(Ordering::SeqCst), 0);
    }
}
