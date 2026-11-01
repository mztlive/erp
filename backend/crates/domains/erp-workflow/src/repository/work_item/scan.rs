//! 同一查询游标按固定批次供给候选，业务授权仍由调用方逐批执行。

use std::num::NonZeroU32;

use futures_util::{TryStream, TryStreamExt};
use mongodb::options::FindOptions;
use mongodb::{Collection, Cursor, SessionCursor};
use persistence_core::{Error, Executor, QueryFilter, Result};

use super::query::{sort_doc, work_item_projection};
use super::{WorkItemFilter, WorkItemRow};
use crate::entity::work_item::WorkItem;

/// 在原执行上下文内连续读取的工作项候选游标。
pub struct WorkItemScan {
    cursor: ScanCursor,
}

/// 事务游标必须在每批读取时重新借用调用方会话。
enum ScanCursor {
    Independent(Cursor<WorkItemRow>),
    Transactional(SessionCursor<WorkItemRow>),
}

impl WorkItemScan {
    /// 打开一次完整排序查询，后续批次不重新执行 skip 查询。
    ///
    /// # 参数
    /// * `collection` - `work_items` 集合。
    /// * `filter` - 与旧候选扫描相同的筛选。
    /// * `batch_size` - 每次服务端取数上限。
    /// * `executor` - 调用方执行器；有会话时绑定该会话。
    ///
    /// # 返回
    /// 返回绑定调用方会话的候选游标，不执行候选计数。
    ///
    /// # 错误
    /// 数据库查询失败时保持原仓储错误分类。
    pub(super) async fn open(
        collection: Collection<WorkItem>,
        filter: &WorkItemFilter,
        batch_size: NonZeroU32,
        executor: &mut dyn Executor,
    ) -> Result<Self> {
        let collection = collection.clone_with_type::<WorkItemRow>();
        let options = FindOptions::builder()
            .sort(sort_doc(filter.sort_by.as_deref(), filter.sort_ascending))
            .batch_size(batch_size.get())
            .projection(work_item_projection())
            .build();
        let query = collection.find(filter.to_doc()).with_options(options);
        let cursor = match executor.session() {
            Some(session) => ScanCursor::Transactional(query.session(session).await?),
            None => ScanCursor::Independent(query.await?),
        };
        Ok(Self { cursor })
    }

    /// 消费至多一个批次，允许调用方在两批之间继续读取授权事实。
    ///
    /// # 参数
    /// `executor` 必须沿用打开游标的执行上下文；`batch_size` 为非零批次大小。
    /// # 返回
    /// 按原查询排序返回下一批；耗尽后返回空集合。
    /// # 错误
    /// 游标读取失败或事务游标失去会话时失败关闭。
    pub async fn next_batch(
        &mut self,
        batch_size: NonZeroU32,
        executor: &mut dyn Executor,
    ) -> Result<Vec<WorkItemRow>> {
        match &mut self.cursor {
            ScanCursor::Independent(cursor) => read_batch(cursor, batch_size).await,
            ScanCursor::Transactional(cursor) => read_session_batch(cursor, batch_size, executor).await,
        }
    }
}

/// 会话游标逐项推进，取消 getMore 时不遗留持有会话的流状态。
async fn read_session_batch(
    cursor: &mut SessionCursor<WorkItemRow>,
    batch_size: NonZeroU32,
    executor: &mut dyn Executor,
) -> Result<Vec<WorkItemRow>> {
    let session = executor.session().ok_or(Error::UnsupportedDeployment("事务工作项游标必须沿用原执行器"))?;
    let mut rows = Vec::with_capacity(batch_size.get() as usize);
    for _ in 0..batch_size.get() {
        if !cursor.advance(session).await? {
            break;
        }
        rows.push(cursor.deserialize_current()?);
    }
    Ok(rows)
}

/// 固定批次消费独立游标，不越过首个读取错误。
async fn read_batch<S>(mut stream: S, batch_size: NonZeroU32) -> Result<Vec<WorkItemRow>>
where
    S: TryStream<Ok = WorkItemRow, Error = mongodb::error::Error> + Unpin,
{
    let mut rows = Vec::with_capacity(batch_size.get() as usize);
    for _ in 0..batch_size.get() {
        let Some(row) = stream.try_next().await? else {
            break;
        };
        rows.push(row);
    }
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use futures_util::stream;
    use mongodb::bson::{deserialize_from_document, doc};
    use mongodb::error::Error as MongoError;

    use super::*;

    /// 查询状态及两种游标的读取 future 都可跨异步执行线程传递。
    #[test]
    fn work_item_scan_and_reads_are_send() {
        /// 约束查询状态可跨线程移动。
        fn assert_send<T: Send>() {}
        /// 约束读取 future 可跨线程调度。
        fn assert_future_send(_: impl std::future::Future + Send) {}
        /// 检查独立游标读取的 Send 边界。
        fn independent(cursor: &mut Cursor<WorkItemRow>, size: NonZeroU32) {
            assert_future_send(read_batch(cursor, size));
        }
        /// 检查沿用执行器的事务游标读取的 Send 边界。
        fn transactional(
            cursor: &mut SessionCursor<WorkItemRow>,
            size: NonZeroU32,
            executor: &mut dyn Executor,
        ) {
            assert_future_send(read_session_batch(cursor, size, executor));
        }
        assert_send::<WorkItemScan>();
        let _ = (independent, transactional);
    }

    /// 使用真实投影反序列化构造候选，测试流式消费不会改变字段或顺序。
    fn row(id: &str) -> WorkItemRow {
        deserialize_from_document(doc! {
            "id": id, "work_item_type": "CARD_FUNDS_DELTA_REVIEW",
            "business_object_type": "receivable_account", "business_object_id": "account",
            "subject_version": "v1", "status": "OPEN", "owner_role": "finance",
            "owner_organization_id": "org", "owner_user_id": "owner",
            "responsibility_actor_ids": [], "assignment_source": "SYSTEM_RULE",
            "priority": "normal", "version": 1_i64, "created_at": 1_i64, "updated_at": 1_i64,
        })
        .unwrap()
    }

    /// 全批、尾批及空批消费都保持游标原顺序，且不重复或丢失候选。
    #[tokio::test]
    async fn batches_preserve_order_and_stop_at_end() {
        let expected = vec![row("c"), row("a"), row("b")];
        let mut source = stream::iter(expected.clone().into_iter().map(Ok::<_, MongoError>));
        let size = NonZeroU32::new(2).unwrap();
        assert_eq!(read_batch(&mut source, size).await.unwrap(), expected[..2]);
        assert_eq!(read_batch(&mut source, size).await.unwrap(), expected[2..]);
        assert!(read_batch(&mut source, size).await.unwrap().is_empty());
    }

    /// 后续读取失败不能将已读取的部分批次报告为成功。
    #[tokio::test]
    async fn batch_read_failure_returns_no_partial_success() {
        let error = MongoError::custom("读取失败");
        let mut source = stream::iter(vec![Ok(row("first")), Err(error), Ok(row("last"))]);
        let size = NonZeroU32::new(2).unwrap();
        assert!(matches!(read_batch(&mut source, size).await, Err(Error::DatabaseError(_))));
        assert_eq!(read_batch(&mut source, size).await.unwrap(), vec![row("last")]);
    }

    /// 前一批成功后出现读取错误，仍整体拒绝该后续批次。
    #[tokio::test]
    async fn later_batch_read_failure_is_propagated() {
        let error = MongoError::custom("读取失败");
        let mut source = stream::iter(vec![Ok(row("first")), Ok(row("second")), Err(error)]);
        let size = NonZeroU32::new(2).unwrap();
        assert_eq!(read_batch(&mut source, size).await.unwrap(), vec![row("first"), row("second")]);
        assert!(matches!(read_batch(&mut source, size).await, Err(Error::DatabaseError(_))));
    }
}
