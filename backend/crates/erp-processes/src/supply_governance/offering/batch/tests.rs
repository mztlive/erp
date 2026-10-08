//! 有界批量的身份与保守错误恢复测试。
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use super::*;

struct Fake {
    identity: String,
    key: String,
    supplier: String,
    invalid: bool,
    replay: bool,
    writes: Arc<AtomicUsize>,
}
#[async_trait]
impl BatchOperation for Fake {
    /// 返回测试业务身份。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回 `identity` 的副本。
    ///
    /// # 错误
    /// 不返回错误。
    fn identity(&self) -> String {
        self.identity.clone()
    }
    /// 返回测试供应商。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回 `supplier`。
    ///
    /// # 错误
    /// 不返回错误。
    fn supplier(&self) -> Option<&str> {
        Some(&self.supplier)
    }
    /// 返回测试命令键。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回可改写的 `key`。
    ///
    /// # 错误
    /// 不返回错误。
    fn key(&mut self) -> &mut String {
        &mut self.key
    }
    /// 模拟规则失败和持久化回执。
    ///
    /// # 参数
    /// * `_` - 未使用的供给流程。
    /// * `_` - 未使用的操作人。
    ///
    /// # 返回
    /// `replay` 为真时返回 `Some(true)`，否则返回 `None`。
    ///
    /// # 错误
    /// `invalid` 为真时返回 `ValidationError`。
    async fn prepare(&self, _: &SupplierOfferingProcess, _: &AuditActor) -> Result<Option<Value>> {
        if self.invalid {
            return Err(Error::ValidationError("订货编码重复".into()));
        }
        Ok(self.replay.then_some(Value::Bool(true)))
    }
    /// 记录实际执行次数。
    ///
    /// # 参数
    /// * `_` - 未使用的供给流程。
    /// * `_` - 未使用的操作人。
    ///
    /// # 返回
    /// 写入计数加一后返回 `true`。
    ///
    /// # 错误
    /// 不返回错误。
    async fn execute(self, _: &SupplierOfferingProcess, _: &AuditActor) -> Result<Value> {
        self.writes.fetch_add(1, Ordering::SeqCst);
        Ok(Value::Bool(true))
    }
}
/// 构建不接触外部服务的测试行。
fn request(count: usize) -> BatchRequest<Fake> {
    BatchRequest {
        validate_only: false,
        rows: (0..count)
            .map(|i| BatchRow {
                row_id: i.to_string(),
                input: Fake {
                    identity: i.to_string(),
                    key: i.to_string(),
                    supplier: "s1".into(),
                    invalid: false,
                    replay: false,
                    writes: Arc::new(AtomicUsize::new(0)),
                },
            })
            .collect(),
    }
}
/// 验证批量容器及错误恢复约束。
#[test]
fn rejects_empty_oversize_duplicates_and_mixed_supplier() {
    assert!(validate_envelope(&mut request(0), "a").is_err());
    assert!(validate_envelope(&mut request(101), "a").is_err());
    assert!(validate_envelope(&mut request(100), "a").is_ok());
    for change in 0..4 {
        let mut batch = request(2);
        match change {
            0 => batch.rows[1].row_id = "0".into(),
            1 => batch.rows[1].input.identity = "0".into(),
            2 => batch.rows[1].input.key = "0".into(),
            _ => batch.rows[1].input.supplier = "s2".into(),
        }
        assert!(validate_envelope(&mut batch, "a").is_err());
    }
}
/// 验证批量容器及错误恢复约束。
#[test]
fn isolates_keys_by_actor_and_hides_internal_messages() {
    let mut a = request(1);
    let mut b = request(1);
    validate_envelope(&mut a, "alice").unwrap();
    validate_envelope(&mut b, "bob").unwrap();
    assert_ne!(a.rows[0].input.key, b.rows[0].input.key);
    let error = Error::Internal("database-password".into());
    assert!(!safe_message(&error).contains("database-password"));
    assert_eq!(failure_status(&error), RowStatus::Unknown);
    assert_eq!(failure_status(&Error::ConflictError("changed".into())), RowStatus::Failed);
}

struct FakeRunner;
#[async_trait]
impl BatchRunner<Fake> for FakeRunner {
    /// 模拟准备，不读取数据库。
    ///
    /// # 参数
    /// * `input` - 测试行。
    ///
    /// # 返回
    /// `replay` 为真时返回 `Some(true)`，否则返回 `None`。
    ///
    /// # 错误
    /// `invalid` 为真时返回 `ValidationError`。
    async fn prepare(&self, input: &Fake) -> Result<Option<Value>> {
        if input.invalid {
            return Err(Error::ValidationError("订货编码重复".into()));
        }
        Ok(input.replay.then_some(Value::Bool(true)))
    }
    /// 模拟写入，不连接外部服务。
    ///
    /// # 参数
    /// * `input` - 测试行；按其 `identity` 决定结果。
    ///
    /// # 返回
    /// 先把写入计数加一。身份不是 `reject` 或 `unknown` 时返回 `true`。
    ///
    /// # 错误
    /// 身份为 `reject` 时返回 `ConflictError`；为 `unknown` 时返回 `Internal`。计数仍已增加。
    async fn execute(&self, input: Fake) -> Result<Value> {
        input.writes.fetch_add(1, Ordering::SeqCst);
        match input.identity.as_str() {
            "reject" => Err(Error::ConflictError("条款版本变化".into())),
            "unknown" => Err(Error::Internal("commit response unavailable".into())),
            _ => Ok(Value::Bool(true)),
        }
    }
}
/// 验证批量预检和逐行执行边界。
#[tokio::test]
async fn preflight_failure_blocks_all_writes_and_replays_stay_locked() {
    let mut batch = request(3);
    let calls = batch.rows[0].input.writes.clone();
    batch.rows[1].input.invalid = true;
    batch.rows[2].input.replay = true;
    let result = run_batch(&FakeRunner, batch).await.unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert_eq!(result.rows[0].status, RowStatus::Ready);
    assert_eq!(result.rows[1].status, RowStatus::Invalid);
    assert_eq!(result.rows[2].status, RowStatus::Succeeded);
}
/// 验证批量预检和逐行执行边界。
#[tokio::test]
async fn validation_does_not_write_and_execution_skips_replays() {
    for validate_only in [true, false] {
        let mut batch = request(2);
        batch.validate_only = validate_only;
        batch.rows[1].input.replay = true;
        let writes = batch.rows[0].input.writes.clone();
        let replay_writes = batch.rows[1].input.writes.clone();
        let result = run_batch(&FakeRunner, batch).await.unwrap();
        assert_eq!(writes.load(Ordering::SeqCst), usize::from(!validate_only));
        assert_eq!(replay_writes.load(Ordering::SeqCst), 0);
        assert_eq!(result.rows[1].status, RowStatus::Succeeded);
    }
}

/// 单行明确失败和结果不明均不得抹去其他行的成功结果。
#[tokio::test]
async fn partial_failure_preserves_successes_and_continues_later_rows() {
    let mut batch = request(4);
    batch.rows[1].input.identity = "reject".into();
    batch.rows[2].input.identity = "unknown".into();
    let result = run_batch(&FakeRunner, batch).await.unwrap();
    let statuses: Vec<_> = result.rows.into_iter().map(|row| row.status).collect();
    assert_eq!(
        statuses,
        vec![RowStatus::Succeeded, RowStatus::Failed, RowStatus::Unknown, RowStatus::Succeeded]
    );
}
