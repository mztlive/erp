//! 选品读取与预检使用独立快照；外部素材准备保持在调用方事务外。
use std::future::Future;
use std::pin::Pin;

use persistence_core::{Executor, Transactional};

use super::SupplierCatalogAdoptionProcess;
use crate::Result;

impl SupplierCatalogAdoptionProcess {
    /// 同一快照执行选品授权与数据库读取；回调不能执行外部 I/O 或开启事务。
    pub(super) async fn with_snapshot<T, F>(&self, read: F) -> Result<T>
    where
        T: Send,
        F: for<'a> FnOnce(Self, &'a mut dyn Executor) -> Pin<Box<dyn Future<Output = Result<T>> + Send + 'a>>
            + Send,
    {
        let process = self.clone();
        self.db.client().with_transaction(move |executor| read(process, executor)).await
    }
}
