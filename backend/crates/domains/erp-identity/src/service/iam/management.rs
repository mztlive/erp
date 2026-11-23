//! 管理用例只保存调用身份，在每次实际读写阶段重新执行统一授权。
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use application_core::AuditActor;
use erp_permissions::Permission;
use mongodb::Database;
use persistence_core::{Executor, Transactional};

use crate::{AccessTarget, AuthorizationService, Result};

type AccessTxFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T>> + Send + 'a>>;

/// 管理用例的入口授权上下文，供预检、事务内读写和列表交付前复验复用。
#[derive(Clone)]
pub(crate) struct ManagementAccess {
    service: Arc<AuthorizationService>,
    actor: AuditActor,
    permission: Permission,
}

impl ManagementAccess {
    /// 进入管理用例时绑定操作人、共享授权服务和固定入口权限。构造只保存检查参数，实际读写阶段重新取证。
    pub(crate) fn new(
        service: Arc<AuthorizationService>,
        actor: AuditActor,
        permission: &Permission,
    ) -> Self {
        Self { service, actor, permission: permission.clone() }
    }

    /// 管理读写前或列表交付前，沿当前快照执行器检查真实账号的入口功能权限。
    pub(crate) async fn require(&self, executor: &mut dyn Executor) -> Result<()> {
        self.service.require(&self.actor, &self.permission, AccessTarget::Operation, executor).await
    }

    /// 在准备密码或命令资料前，用独立事务预检入口功能权限；实际写入仍须在写事务内重新检查。
    pub(crate) async fn preflight(&self, db: &Database) -> Result<()> {
        self.clone().read_tx(db, |_| Box::pin(async { Ok(()) })).await
    }

    /// 在同一事务内核验入口权限后执行管理读取。
    pub(crate) async fn read_tx<T, F>(self, db: &Database, query: F) -> Result<T>
    where
        T: Send + 'static,
        F: for<'a> FnOnce(&'a mut dyn Executor) -> AccessTxFuture<'a, T> + Send + 'static,
    {
        db.client()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    self.require(executor).await?;
                    query(executor).await
                })
            })
            .await
    }

    /// 在同一事务内于读取前后两次核验入口权限，确保交付前资格未失效。
    pub(crate) async fn verified_read_tx<T, F>(self, db: &Database, query: F) -> Result<T>
    where
        T: Send + 'static,
        F: for<'a> FnOnce(&'a mut dyn Executor) -> AccessTxFuture<'a, T> + Send + 'static,
    {
        db.client()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    self.require(executor).await?;
                    let result = query(executor).await?;
                    self.require(executor).await?;
                    Ok(result)
                })
            })
            .await
    }
}
