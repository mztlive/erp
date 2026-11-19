//! 管理用例只保存调用身份，在每次实际读写阶段重新执行统一授权。
use std::sync::Arc;

use application_core::AuditActor;
use erp_permissions::Permission;
use mongodb::Database;
use persistence_core::{Executor, Transactional};

use crate::{AccessTarget, AuthorizationService, Result};

/// 管理用例的入口授权上下文，供预检、事务内读写和列表交付前复验复用。
#[derive(Clone)]
pub(crate) struct ManagementAccess {
    // 应用装配的共享授权服务。
    service: Arc<AuthorizationService>,
    // 本次操作的真实账号身份。
    actor: AuditActor,
    // 管理用例固定的无对象入口权限。
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
    /// 身份、资格、执行边界及授权设施错误均原样返回。
    pub(crate) async fn require(&self, executor: &mut dyn Executor) -> Result<()> {
        self.service.require(&self.actor, &self.permission, AccessTarget::Operation, executor).await
    }

    /// 在准备密码或命令资料前，用独立事务预检入口功能权限。
    /// 预检只决定能否继续准备；实际写入仍须在写事务内重新检查。
    pub(crate) async fn preflight(&self, db: &Database) -> Result<()> {
        let access = self.clone();
        db.client()
            .with_transaction(move |executor| Box::pin(async move { access.require(executor).await }))
            .await
    }
}
