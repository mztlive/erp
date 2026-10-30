//! 真实编排的内存持久端口替身；不冒充 Mongo 并发或事务验收。
use std::collections::BTreeMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use erp_support::entity::scheduled_run::ScheduledState;
use persistence_core::{Error, Result as StoreResult};

use super::*;

#[derive(Default)]
struct MemoryStore {
    rows: Mutex<BTreeMap<String, ScheduledRun>>,
    fail_finish: AtomicBool,
    insert_failure: AtomicUsize,
}
#[async_trait]
impl ScheduledRunStore for MemoryStore {
    async fn find(&self, key: &str) -> StoreResult<Option<ScheduledRun>> {
        Ok(self.rows.lock().unwrap().get(key).cloned())
    }
    async fn insert(&self, run: &ScheduledRun) -> StoreResult<()> {
        if self.insert_failure.load(Ordering::SeqCst) == 1 {
            return Err(Error::OptimisticLockingError);
        }
        let mut rows = self.rows.lock().unwrap();
        if rows.contains_key(&run.base.id)
            || rows.values().any(|r| {
                r.request.app == run.request.app
                    && r.state != ScheduledState::Completed
                    && r.request.job_id == run.request.job_id
            })
        {
            return Err(Error::OptimisticLockingError);
        }
        rows.insert(run.base.id.clone(), run.clone());
        if self.insert_failure.load(Ordering::SeqCst) == 2 {
            return Err(Error::OptimisticLockingError);
        }
        Ok(())
    }
    async fn job_busy(&self, app: &str, job_id: i32) -> StoreResult<bool> {
        Ok(self.rows.lock().unwrap().values().any(|r| {
            r.request.app == app && r.request.job_id == job_id && r.state != ScheduledState::Completed
        }))
    }
    async fn claim(&self, app: &str, owner: &str) -> StoreResult<Option<ScheduledRun>> {
        let mut rows = self.rows.lock().unwrap();
        let Some(run) =
            rows.values_mut().find(|r| r.request.app == app && r.state == ScheduledState::Accepted)
        else {
            return Ok(None);
        };
        run.state = ScheduledState::Running;
        run.owner = Some(owner.into());
        Ok(Some(run.clone()))
    }
    async fn finish(&self, run: &ScheduledRun, code: i32, message: &str) -> StoreResult<()> {
        if self.fail_finish.load(Ordering::SeqCst) {
            return Err(Error::OptimisticLockingError);
        }
        let mut rows = self.rows.lock().unwrap();
        let row = rows.get_mut(&run.base.id).unwrap();
        row.state = ScheduledState::Completed;
        row.handle_code = code;
        row.handle_msg = message.into();
        Ok(())
    }
    async fn recover(&self, app: &str, owner: &str) -> StoreResult<()> {
        for r in self.rows.lock().unwrap().values_mut() {
            if r.request.app == app && r.state == ScheduledState::Running && r.owner.as_deref() != Some(owner)
            {
                r.state = ScheduledState::Completed;
                r.handle_code = 500;
            }
        }
        Ok(())
    }
    async fn callbacks(&self, app: &str, now: i64) -> StoreResult<Vec<ScheduledRun>> {
        Ok(self
            .rows
            .lock()
            .unwrap()
            .values()
            .filter(|r| {
                r.request.app == app
                    && r.state == ScheduledState::Completed
                    && !r.callback_done
                    && r.callback_after <= now
            })
            .take(20)
            .cloned()
            .collect())
    }
    async fn callback_result(&self, run: &ScheduledRun, success: bool, now: i64) -> StoreResult<()> {
        let mut rows = self.rows.lock().unwrap();
        let r = rows.get_mut(&run.base.id).unwrap();
        r.callback_done = success;
        r.callback_after = now + 30;
        Ok(())
    }
}
#[derive(Default)]
struct Work {
    calls: AtomicUsize,
    fail: AtomicBool,
}
#[async_trait]
impl ScheduledWork for Work {
    async fn run(&self, _: ScheduledHandler) -> Result<usize, &'static str> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if self.fail.load(Ordering::SeqCst) { Err("执行失败") } else { Ok(3) }
    }
}
#[derive(Default)]
struct Callback {
    calls: AtomicUsize,
    succeed: AtomicBool,
}
#[async_trait]
impl ScheduleCallback for Callback {
    async fn send(&self, _: &ScheduledRun) -> bool {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.succeed.load(Ordering::SeqCst)
    }
}
fn request(log_id: i64) -> ScheduledRequest {
    ScheduledRequest {
        app: "test".into(),
        job_id: 1,
        log_id,
        log_time: 1_000_000,
        handler: ScheduledHandler::Tracking,
    }
}
fn process(
    store: Arc<MemoryStore>,
    work: Arc<Work>,
    callback: Arc<Callback>,
    owner: &str,
) -> ScheduledExecutionProcess {
    ScheduledExecutionProcess::new("test".into(), owner.into(), store, work, callback)
}

#[tokio::test]
async fn accept_deduplicates_busy_and_mismatched_identity_before_any_work() {
    let store = Arc::new(MemoryStore::default());
    let work = Arc::new(Work::default());
    let callback = Arc::new(Callback::default());
    let p = process(store.clone(), work.clone(), callback, "one");
    p.accept(request(1), 1_000_000).await.unwrap();
    p.accept(request(1), 1_000_000).await.unwrap();
    assert!(matches!(p.accept(request(2), 1_000_000).await, Err(DispatchError::Busy)));
    let mut different = request(1);
    different.log_time += 1;
    assert!(matches!(p.accept(different, 1_000_000).await, Err(DispatchError::Identity)));
    assert_eq!(work.calls.load(Ordering::SeqCst), 0);
    p.run_next().await.unwrap();
    assert!(!p.run_next().await.unwrap());
    p.accept(request(1), 1_000_000).await.unwrap();
    assert!(!p.run_next().await.unwrap());
    assert_eq!(work.calls.load(Ordering::SeqCst), 1);
    assert_eq!(store.find("test:1").await.unwrap().unwrap().handle_code, 200);
}

#[tokio::test]
async fn failed_callbacks_retry_without_replaying_business_and_preserve_backoff() {
    let store = Arc::new(MemoryStore::default());
    let work = Arc::new(Work::default());
    let callback = Arc::new(Callback::default());
    let p = process(store.clone(), work.clone(), callback.clone(), "one");
    p.accept(request(1), 1_000_000).await.unwrap();
    p.run_next().await.unwrap();
    p.callbacks(1000).await.unwrap();
    p.callbacks(1010).await.unwrap();
    assert_eq!(callback.calls.load(Ordering::SeqCst), 1);
    callback.succeed.store(true, Ordering::SeqCst);
    let restarted = process(store, work.clone(), callback.clone(), "two");
    restarted.recover().await.unwrap();
    restarted.callbacks(1030).await.unwrap();
    restarted.callbacks(1060).await.unwrap();
    assert_eq!(callback.calls.load(Ordering::SeqCst), 2);
    assert_eq!(work.calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn uncertain_completion_is_not_replayed_after_restart() {
    let store = Arc::new(MemoryStore::default());
    let work = Arc::new(Work::default());
    let callback = Arc::new(Callback::default());
    let p = process(store.clone(), work.clone(), callback.clone(), "one");
    p.accept(request(1), 1_000_000).await.unwrap();
    store.fail_finish.store(true, Ordering::SeqCst);
    assert!(p.run_next().await.is_err());
    store.fail_finish.store(false, Ordering::SeqCst);
    let restarted = process(store.clone(), work.clone(), callback, "two");
    restarted.recover().await.unwrap();
    assert!(!restarted.run_next().await.unwrap());
    assert_eq!(store.find("test:1").await.unwrap().unwrap().handle_code, 500);
    assert_eq!(work.calls.load(Ordering::SeqCst), 1);
    restarted.accept(request(2), 1_000_000).await.unwrap();
    work.fail.store(true, Ordering::SeqCst);
    restarted.run_next().await.unwrap();
    assert_eq!(store.find("test:2").await.unwrap().unwrap().handle_code, 500);
}

#[tokio::test]
async fn stale_or_invalid_requests_never_persist() {
    let store = Arc::new(MemoryStore::default());
    let p = process(store.clone(), Arc::new(Work::default()), Arc::new(Callback::default()), "one");
    assert!(matches!(p.accept(request(1), 100_000_000).await, Err(DispatchError::Stale)));
    let mut r = request(1);
    r.job_id = 0;
    assert!(matches!(p.accept(r, 1_000_000).await, Err(DispatchError::Stale)));
    assert!(store.rows.lock().unwrap().is_empty());
}

#[tokio::test]
async fn accepted_work_survives_restart_and_runs_once() {
    let store = Arc::new(MemoryStore::default());
    let work = Arc::new(Work::default());
    let callback = Arc::new(Callback::default());
    let before = process(store.clone(), work.clone(), callback.clone(), "one");
    before.accept(request(1), 1_000_000).await.unwrap();
    assert_eq!(work.calls.load(Ordering::SeqCst), 0);
    drop(before);
    let after = process(store.clone(), work.clone(), callback, "two");
    after.recover().await.unwrap();
    assert_eq!(store.find("test:1").await.unwrap().unwrap().state, ScheduledState::Accepted);
    assert!(after.run_next().await.unwrap());
    after.accept(request(1), 100_000_000).await.unwrap();
    assert!(!after.run_next().await.unwrap());
    assert_eq!(work.calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn persistence_failure_rejects_but_lost_acknowledgement_recovers_original_receipt() {
    let store = Arc::new(MemoryStore::default());
    let work = Arc::new(Work::default());
    let p = process(store.clone(), work.clone(), Arc::new(Callback::default()), "one");
    store.insert_failure.store(1, Ordering::SeqCst);
    assert!(matches!(p.accept(request(1), 1_000_000).await, Err(DispatchError::Store(_))));
    assert!(!p.run_next().await.unwrap());
    store.insert_failure.store(2, Ordering::SeqCst);
    p.accept(request(1), 1_000_000).await.unwrap();
    assert_eq!(store.rows.lock().unwrap().len(), 1);
    assert_eq!(work.calls.load(Ordering::SeqCst), 0);
    p.run_next().await.unwrap();
    p.accept(request(1), 1_000_000).await.unwrap();
    assert!(!p.run_next().await.unwrap());
    assert_eq!(work.calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn different_jobs_share_a_handler_but_only_the_same_job_is_blocked() {
    let store = Arc::new(MemoryStore::default());
    let work = Arc::new(Work::default());
    let p = process(store.clone(), work.clone(), Arc::new(Callback::default()), "one");
    let mut first = request(1);
    first.job_id = 123;
    p.accept(first.clone(), 1_000_000).await.unwrap();
    assert!(p.busy(123).await.unwrap());
    let mut same_job_other_handler = request(3);
    same_job_other_handler.job_id = 123;
    same_job_other_handler.handler = ScheduledHandler::Catalog;
    assert!(matches!(p.accept(same_job_other_handler, 1_000_000).await, Err(DispatchError::Busy)));

    assert!(!p.busy(456).await.unwrap());
    let mut another = request(2);
    another.job_id = 456;
    p.accept(another.clone(), 1_000_000).await.unwrap();
    assert!(p.busy(456).await.unwrap());
    let mut mismatched = first;
    mismatched.job_id = 456;
    assert!(matches!(p.accept(mismatched, 1_000_000).await, Err(DispatchError::Identity)));
    assert!(p.run_next().await.unwrap());
    assert!(!p.busy(123).await.unwrap());
    p.accept(another, 1_000_000).await.unwrap();
    assert!(p.run_next().await.unwrap());
    assert_eq!(work.calls.load(Ordering::SeqCst), 2);
    assert_eq!(store.find("test:2").await.unwrap().unwrap().request.job_id, 456);
}
