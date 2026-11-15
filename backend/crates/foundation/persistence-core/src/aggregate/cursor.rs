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
    /// 打开按固定管道过滤后的聚合游标。
    /// # 参数
    /// collection、pipeline 为拥有领域的集合和完整管道，batch_size 为批次上限，executor 为原执行器。
    /// # 返回
    /// 保留原会话的游标。
    /// # 错误
    /// 数据库或解码错误原样传播。
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

    /// 经原执行器读取下一有界批次。
    /// # 参数
    /// batch_size 为返回上限，executor 为打开游标的执行器。
    /// # 返回
    /// 至多 batch_size 条记录；空集合表示耗尽。
    /// # 错误
    /// 事务游标失去会话、查询或解码失败时拒绝。
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
