//! 直接执行生产 runner 的替身证据，不连接数据库。
use std::cell::RefCell;
use std::collections::VecDeque;

use super::*;
use crate::Error;

struct TestExecutor {
    _identity: u8,
}

impl Executor for TestExecutor {
    fn session(&mut self) -> Option<&mut mongodb::ClientSession> {
        None
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Step {
    Bound,
    Authorize,
    Domain,
    Transition,
    TaskWrite,
    Result,
    Receipt,
    NoTask,
}

struct TestPort {
    fail: Option<Step>,
    seen: Vec<Step>,
    executors: Vec<usize>,
}

impl TestPort {
    fn new(fail: Option<Step>) -> Self {
        Self { fail, seen: Vec::new(), executors: Vec::new() }
    }

    fn step(&mut self, step: Step, executor: Option<&mut dyn Executor>) -> Result<()> {
        self.seen.push(step);
        if let Some(executor) = executor {
            self.executors.push(executor as *mut dyn Executor as *mut () as usize);
        }
        if self.fail == Some(step) {
            return Err(Error::ConflictError(format!("stop:{step:?}")));
        }
        Ok(())
    }
}

#[async_trait]
impl TaskCommandPort for TestPort {
    type Item = String;
    type Fact = String;
    type Output = &'static str;

    async fn load_bound(&mut self, executor: &mut dyn Executor) -> Result<Self::Item> {
        self.step(Step::Bound, Some(executor))?;
        Ok("bound".to_string())
    }
    async fn authorize(&mut self, _: &Self::Item, executor: &mut dyn Executor) -> Result<()> {
        self.step(Step::Authorize, Some(executor))
    }
    async fn apply_domain(&mut self, executor: &mut dyn Executor) -> Result<Self::Fact> {
        self.step(Step::Domain, Some(executor))?;
        Ok("version-2".to_string())
    }
    fn transition(&mut self, item: &mut Self::Item, fact: &Self::Fact) -> Result<()> {
        self.step(Step::Transition, None)?;
        *item = fact.clone();
        Ok(())
    }
    async fn persist_task(&mut self, item: &mut Self::Item, executor: &mut dyn Executor) -> Result<()> {
        assert_eq!(item, "version-2");
        self.step(Step::TaskWrite, Some(executor))
    }
    fn result(&mut self, _: &Self::Fact) -> Result<Self::Output> {
        self.step(Step::Result, None)?;
        Ok("result")
    }
    async fn receipt(&mut self, _: &Self::Fact, executor: &mut dyn Executor) -> Result<()> {
        self.step(Step::Receipt, Some(executor))
    }
}

#[async_trait]
impl DirectCommandPort for TestPort {
    type Fact = String;
    type Output = &'static str;

    async fn ensure_no_task(&mut self, executor: &mut dyn Executor) -> Result<()> {
        self.step(Step::NoTask, Some(executor))
    }
    async fn apply_domain(&mut self, executor: &mut dyn Executor) -> Result<Self::Fact> {
        self.step(Step::Domain, Some(executor))?;
        Ok("appended".to_string())
    }
    async fn receipt(&mut self, _: &Self::Fact, executor: &mut dyn Executor) -> Result<()> {
        self.step(Step::Receipt, Some(executor))
    }
    fn result(&mut self, _: Self::Fact) -> Self::Output {
        self.seen.push(Step::Result);
        "result"
    }
}

fn check_trace(port: &TestPort, expected: &[Step], identity: usize) {
    assert_eq!(port.seen, expected);
    assert!(!port.executors.is_empty());
    assert!(port.executors.iter().all(|actual| *actual == identity));
}

#[tokio::test]
async fn action_uses_one_executor_and_stops_at_every_failed_step() {
    let steps = [
        Step::Bound,
        Step::Authorize,
        Step::Domain,
        Step::Transition,
        Step::TaskWrite,
        Step::Result,
        Step::Receipt,
    ];
    for stop in 0..=steps.len() {
        let mut executor = TestExecutor { _identity: 1 };
        let identity = &mut executor as *mut TestExecutor as usize;
        let mut port = TestPort::new(steps.get(stop).copied());
        let result = run_action(&mut port, &mut executor).await;
        assert_eq!(result.is_ok(), stop == steps.len());
        check_trace(&port, &steps[..(stop + 1).min(steps.len())], identity);
    }
}

#[tokio::test]
async fn completion_receipt_follows_task_write_and_precedes_result() {
    let steps = [
        Step::Bound,
        Step::Authorize,
        Step::Domain,
        Step::Transition,
        Step::TaskWrite,
        Step::Receipt,
        Step::Result,
    ];
    for stop in 0..=steps.len() {
        let mut executor = TestExecutor { _identity: 2 };
        let identity = &mut executor as *mut TestExecutor as usize;
        let mut port = TestPort::new(steps.get(stop).copied());
        let result = run_completion(&mut port, &mut executor).await;
        assert_eq!(result.is_ok(), stop == steps.len());
        check_trace(&port, &steps[..(stop + 1).min(steps.len())], identity);
    }
}

#[tokio::test]
async fn direct_task_guard_precedes_domain_read_and_each_failure_stops() {
    let steps = [Step::NoTask, Step::Domain, Step::Receipt, Step::Result];
    for stop in 0..steps.len() {
        let mut executor = TestExecutor { _identity: 3 };
        let identity = &mut executor as *mut TestExecutor as usize;
        let mut port = TestPort::new(if stop == 3 { None } else { Some(steps[stop]) });
        let result = run_direct(&mut port, &mut executor).await;
        assert_eq!(result.is_ok(), stop == 3);
        check_trace(&port, &steps[..=stop], identity);
    }
}

#[tokio::test]
async fn committed_receipt_bypasses_closed_task_and_preparation_path() {
    let trace = RefCell::new(Vec::new());
    let result = execute_with_receipt(
        || {
            trace.borrow_mut().push("receipt");
            std::future::ready(Ok(Some("committed")))
        },
        || {
            trace.borrow_mut().push("rbac/prepared/closed-task");
            std::future::ready(Err(Error::ConflictError("任务已不再开放".to_string())))
        },
    )
    .await
    .unwrap();
    assert_eq!(result, "committed");
    assert_eq!(*trace.borrow(), ["receipt"]);
}

#[tokio::test]
async fn any_transaction_error_recovers_once_and_missing_receipt_keeps_original_error() {
    for recovered in [Some("committed"), None] {
        let trace = RefCell::new(Vec::new());
        let receipts = RefCell::new(VecDeque::from([None, recovered]));
        let result = execute_with_receipt(
            || {
                trace.borrow_mut().push("receipt");
                std::future::ready(Ok(receipts.borrow_mut().pop_front().unwrap()))
            },
            || {
                trace.borrow_mut().push("transaction");
                std::future::ready(Err(Error::Forbidden("original-auth-error".to_string())))
            },
        )
        .await;
        assert_eq!(*trace.borrow(), ["receipt", "transaction", "receipt"]);
        match recovered {
            Some(value) => assert_eq!(result.unwrap(), value),
            None => {
                assert!(matches!(result, Err(Error::Forbidden(message)) if message == "original-auth-error"))
            },
        }
    }
}

#[tokio::test]
async fn replay_read_error_stops_initially_or_replaces_transaction_error_on_recovery() {
    for initial in [true, false] {
        let reads = RefCell::new(0);
        let writes = RefCell::new(0);
        let result: Result<&str> = execute_with_receipt(
            || {
                *reads.borrow_mut() += 1;
                std::future::ready(if initial || *reads.borrow() == 2 {
                    Err(Error::Internal("receipt-read".to_string()))
                } else {
                    Ok(None)
                })
            },
            || {
                *writes.borrow_mut() += 1;
                std::future::ready(Err(Error::Forbidden("transaction".to_string())))
            },
        )
        .await;
        assert!(matches!(result, Err(Error::Internal(message)) if message == "receipt-read"));
        assert_eq!(*reads.borrow(), if initial { 1 } else { 2 });
        assert_eq!(*writes.borrow(), if initial { 0 } else { 1 });
    }
}
