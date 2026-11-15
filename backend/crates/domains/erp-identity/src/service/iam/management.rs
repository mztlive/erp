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
    /// 保存共享服务及固定入口，不保存授权结果。
    /// # 参数
    /// service 为共享实例，actor 为当前账号，permission 为调用用例的固定权限。
    /// # 返回
    /// 每次执行时重新取证的管理上下文。
    /// # 错误
    /// 构造不执行授权或数据库读取。
    pub(crate) fn new(
        service: Arc<AuthorizationService>,
        actor: AuditActor,
        permission: &Permission,
    ) -> Self {
        Self { service, actor, permission: permission.clone() }
    }

    /// 在原事务内检查真实账号及入口功能资格。
    /// # 参数
    /// executor 为本次读取阶段或实际写事务。
    /// # 返回
    /// 当前完整功能资格成立时成功。
    /// # 错误
    /// 身份、功能资格、执行边界或取证失败时原样拒绝。
    pub(crate) async fn require(&self, executor: &mut dyn Executor) -> Result<()> {
        self.service.require(&self.actor, &self.permission, AccessTarget::Operation, executor).await
    }

    /// 事务外准备业务事实前检查入口，实际写事务仍须重新检查。
    /// # 参数
    /// db 为业务写入使用的同一数据库。
    /// # 返回
    /// 预检时点的功能资格成立时成功；不返回可复用凭证。
    /// # 错误
    /// 当前资格或事务设施失败时拒绝。
    pub(crate) async fn preflight(&self, db: &Database) -> Result<()> {
        let access = self.clone();
        db.client()
            .with_transaction(move |executor| Box::pin(async move { access.require(executor).await }))
            .await
    }
}
