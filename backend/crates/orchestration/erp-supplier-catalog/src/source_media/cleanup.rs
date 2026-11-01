//! 只补偿本次新上传且能证明未被提交的对象；未知提交始终保留。

use std::sync::Arc;
use std::time::Duration;

use persistence_core::Error as PersistenceError;
use tokio::time::{Instant, timeout_at};

use super::SourceImageStore;
use crate::{Error, Result};

/// 本次新对象的最小补偿清单；不包含从已有目录复用的资产。
#[derive(Clone, Default)]
pub struct SourceMediaCleanup {
    store: Option<Arc<dyn SourceImageStore>>,
    object_keys: Vec<String>,
}

impl SourceMediaCleanup {
    pub(super) fn new(store: Arc<dyn SourceImageStore>) -> Self {
        Self { store: Some(store), object_keys: Vec::new() }
    }

    pub(super) fn track(&mut self, key: String) {
        self.object_keys.push(key);
    }

    pub(super) fn checkpoint(&self) -> usize {
        self.object_keys.len()
    }

    pub(super) async fn discard_since(&self, checkpoint: usize) {
        self.discard_keys(&self.object_keys[checkpoint..]).await;
    }

    /// 清理已证明未被业务事务接纳的本次对象。
    /// # 参数
    /// 无；调用方必须已经确定尚未开始事务、事务回滚或幂等重放未使用本批次。
    /// # 返回
    /// 尽力清理；失败登记安全日志，不覆盖原业务错误。
    /// # 错误
    /// 不返回错误；提交结果未知或核查失败时禁止调用。
    pub async fn discard_uncommitted(&self) {
        self.discard_keys(&self.object_keys).await;
    }

    /// 仅根据原始事务结果执行补偿，未知提交与成功提交保留对象。
    /// # 参数
    /// `outcome` 必须是事务入口直接返回的结果，禁止传入提交后的读取或返回值构造结果。
    /// # 返回
    /// 明确失败时尽力清理；提交成功或未知时不删除任何对象。
    /// # 错误
    /// 不返回错误；也不以一次只读查询查不到为删除依据。
    pub async fn after_transaction<T>(&self, outcome: &Result<T>) {
        if outcome.as_ref().is_err_and(|error| !unknown_commit(error)) {
            self.discard_uncommitted().await;
        }
    }

    async fn discard_keys(&self, keys: &[String]) {
        let Some(store) = &self.store else { return };
        let deadline = Instant::now() + Duration::from_secs(30);
        for key in keys {
            if Instant::now() >= deadline {
                tracing::warn!("供应商未引用图片对象清理超时，剩余对象需核对本次素材准备记录");
                break;
            }
            let until = deadline.min(Instant::now() + Duration::from_secs(5));
            if !matches!(timeout_at(until, store.delete(key)).await, Ok(Ok(()))) {
                tracing::warn!("供应商商品未引用的新图片对象清理失败，请核对本次素材准备记录");
            }
        }
    }
}

fn unknown_commit(error: &Error) -> bool {
    matches!(
        error,
        Error::OutcomeUnknown(_) | Error::RepositoryError(PersistenceError::CommitOutcomeUnknown(_))
    )
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use async_trait::async_trait;
    use mongodb::error::Error as MongoError;
    use persistence_core::Error as PersistenceError;

    use super::*;

    #[derive(Default)]
    struct Objects(Mutex<Vec<String>>);

    #[async_trait]
    impl SourceImageStore for Objects {
        async fn save(&self, _key: &str, _bytes: &[u8], _content_type: &str) -> Result<()> {
            Ok(())
        }

        async fn delete(&self, key: &str) -> Result<()> {
            self.0.lock().unwrap().push(key.into());
            Ok(())
        }
    }

    fn cleanup() -> (SourceMediaCleanup, Arc<Objects>) {
        let objects = Arc::new(Objects::default());
        let mut cleanup = SourceMediaCleanup::new(objects.clone());
        cleanup.track("supplier-source-media/new.png".into());
        (cleanup, objects)
    }

    #[tokio::test]
    async fn definite_failure_discards_only_new_objects() {
        let (cleanup, objects) = cleanup();
        let outcome: Result<()> = Err(Error::ConflictError("目录版本变化，事务已回滚".into()));
        cleanup.after_transaction(&outcome).await;
        assert_eq!(*objects.0.lock().unwrap(), ["supplier-source-media/new.png"]);
    }

    #[tokio::test]
    async fn committed_and_unknown_outcomes_never_discard_objects() {
        let (cleanup, objects) = cleanup();
        cleanup.after_transaction(&Ok(())).await;
        let unknown: Result<()> = Err(Error::OutcomeUnknown(PersistenceError::CommitOutcomeUnknown(
            MongoError::custom("commit unknown"),
        )));
        cleanup.after_transaction(&unknown).await;
        let wrapped: Result<()> = Err(Error::RepositoryError(PersistenceError::CommitOutcomeUnknown(
            MongoError::custom("unknown commit still unresolved"),
        )));
        cleanup.after_transaction(&wrapped).await;
        assert!(objects.0.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn preparation_failure_can_discard_without_a_database_query() {
        let (mut cleanup, objects) = cleanup();
        let point = cleanup.checkpoint();
        cleanup.track("supplier-source-media/failed.png".into());
        cleanup.discard_since(point).await;
        assert_eq!(*objects.0.lock().unwrap(), ["supplier-source-media/failed.png"]);
    }
}
