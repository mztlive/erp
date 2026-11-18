//! 管理用例只保存调用身份，在每次实际读写阶段重新执行统一授权。
use std::sync::Arc;

use application_core::AuditActor;
use erp_permissions::Permission;
use mongodb::Database;
use persistence_core::{Executor, Transactional};

use crate::{AccessTarget, AuthorizationService, Result};

#[derive(Clone)]
pub(crate) struct ManagementAccess {
    service: Arc<AuthorizationService>,
    actor: AuditActor,
    permission: Permission,
}

impl ManagementAccess {
    // 为管理用例保存当前账号、共享授权服务和固定入口权限。构造不读库，不缓存授权结果；每个实际读写阶段重新取证。
    pub(crate) fn new(
        service: Arc<AuthorizationService>,
        actor: AuditActor,
        permission: &Permission,
    ) -> Self {
        Self { service, actor, permission: permission.clone() }
    }

    // 管理读取或写入前，沿本次快照执行器检查当前真实账号的固定入口功能权限。身份、资格、执行边界或设施故障均原样拒绝。
    pub(crate) async fn require(&self, executor: &mut dyn Executor) -> Result<()> {
        self.service.require(&self.actor, &self.permission, AccessTarget::Operation, executor).await
    }

    // 在事务外准备密码或命令资料前先核对管理入口功能资格。预检成功不签发凭证，真正写入时仍须用写事务执行器重新检查。
    pub(crate) async fn preflight(&self, db: &Database) -> Result<()> {
        let access = self.clone();
        db.client()
            .with_transaction(move |executor| Box::pin(async move { access.require(executor).await }))
            .await
    }
}
