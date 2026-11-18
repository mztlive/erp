//! 在持久化设施内部借用会话的有界聚合游标。
use std::num::NonZeroU32;

use futures_util::TryStreamExt;
use mongodb::bson::Document;
use mongodb::options::AggregateOptions;
use mongodb::{Collection, Cursor, SessionCursor};
use serde::de::DeserializeOwned;

use crate::executor::session;
use crate::{Error, ExecutionBoundary, Executor, Result};

/// 只能通过原执行器推进的聚合结果；授权阶段由拥有领域检查。
pub struct RowCursor<T> {
    cursor: CursorKind<T>,
    boundary: Option<ExecutionBoundary>,
}

enum CursorKind<T> {
    Independent(Cursor<T>),
    Transactional(SessionCursor<T>),
}

impl<T: DeserializeOwned + Send + Sync> RowCursor<T> {
    /// 为不宜一次载入的聚合结果打开游标，使用领域的完整管道和指定驱动批大小。
    /// 记录已有的读取边界，事务查询留在原会话，供后续分批读取。
    pub async fn open<S: Send + Sync>(
        collection: &Collection<S>,
        pipeline: Vec<Document>,
        batch_size: NonZeroU32,
        executor: &mut dyn Executor,
    ) -> Result<Self> {
        let operation = collection
            .aggregate(pipeline)
            .with_type::<T>()
            .with_options(AggregateOptions::builder().batch_size(batch_size.get()).build());

        let boundary = executor.boundary();
        let cursor = match session(executor) {
            Some(session) => CursorKind::Transactional(operation.session(session).await?),
            None => CursorKind::Independent(operation.await?),
        };
        Ok(Self { cursor, boundary })
    }

    /// 沿用打开游标时的执行器读取至多指定条数，空批表示耗尽。
    /// 若打开时记录的边界已失效或事务游标缺少会话，拒绝续读；查询和解码错误向上返回。
    pub async fn next_batch(
        &mut self,
        batch_size: NonZeroU32,
        executor: &mut dyn Executor,
    ) -> Result<Vec<T>> {
        if self.boundary.as_ref().is_some_and(|boundary| !boundary.is_current(executor)) {
            return Err(Error::UnsupportedDeployment("聚合游标不能跨事务或读取阶段使用"));
        }

        let mut rows = Vec::with_capacity(batch_size.get() as usize);
        match &mut self.cursor {
            CursorKind::Independent(cursor) => {
                for _ in 0..batch_size.get() {
                    let Some(row) = cursor.try_next().await? else { break };
                    rows.push(row);
                }
            },
            CursorKind::Transactional(cursor) => {
                let session = session(executor).ok_or(Error::UnsupportedDeployment("事务游标失去原会话"))?;
                for _ in 0..batch_size.get() {
                    if !cursor.advance(session).await? {
                        break;
                    }
                    rows.push(cursor.deserialize_current()?);
                }
            },
        }
        Ok(rows)
    }
}
