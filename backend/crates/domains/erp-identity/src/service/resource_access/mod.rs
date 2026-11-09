//! 统一读取判定及按来源授权管理；业务写入必须在原 Executor 内重新解析。
mod command;
mod queue;
use std::sync::Arc;

use application_core::AuditActor;
use mongodb::Database;
use persistence_core::Executor;
pub use queue::{GrantQueueHealth, GrantRecompute, GrantRule};

use crate::entity::resource_access::ResourceKind;
pub use crate::entity::resource_access::SourceUpdate;
use crate::entity::resource_access::query::{
    AuthorizedResourceAccess, AuthorizedResourceQuery, ResourceTargets, scope_version,
};
pub use crate::ports::ResourceGrantFacts;
use crate::repository::resource_grants;
use crate::service::access_control::resolve::DataScopeService;
use crate::{Error, Permission, Result, SharedRbacService};

#[derive(Clone)]
pub struct ResourceAccessService {
    db: Database,
    rbac: SharedRbacService,
    facts: Arc<dyn ResourceGrantFacts>,
}
impl ResourceAccessService {
    /// 装配身份数据库、权限服务和可信业务事实端口。
    /// # 参数
    /// db、rbac 为现有身份设施；facts 为组合根适配器。
    /// # 返回
    /// 统一授权服务。
    /// # 错误
    /// 不返回错误；没有事实端口不得启用管理写入。
    pub fn new(db: Database, rbac: SharedRbacService, facts: Arc<dyn ResourceGrantFacts>) -> Self {
        Self { db, rbac, facts }
    }
    /// 检查入口并生成列表查询约束，只读取账号资源修订号，不装载授权 ID。
    /// # 参数
    /// actor 为身份；entry_permission 为已解析的服务端入口权限；resource 为真实来源类型；executor 为调用方事务。
    /// # 返回
    /// 供业务仓储编译数据库授权条件的不可变约束。
    /// # 错误
    /// 身份、功能失效、未知入口或数据库失败时拒绝。
    pub async fn resolve(
        &self,
        actor: &AuditActor,
        entry_permission: &Permission,
        resource: ResourceKind,
        executor: &mut dyn Executor,
    ) -> Result<AuthorizedResourceQuery> {
        let source_action = resource.scope_action(entry_permission)?;
        let source_permission = Permission::parse(format!("{}:{source_action}", resource.code()))?;
        let base = DataScopeService::new(self.db.clone(), self.rbac.clone())
            .resolve_source_scope(actor, entry_permission, &source_permission, executor)
            .await?;
        let grant_revision = resource_grants::revision(&self.db, actor.id(), resource, executor).await?;
        let scope_version = scope_version(&base.scope_version, entry_permission, resource, grant_revision);
        Ok(AuthorizedResourceQuery {
            user_id: actor.id().into(),
            resource,
            scope: base.scope,
            scope_version,
            grant_revision,
        })
    }

    /// 解析本次单条或批量目标的读取资格，不读取用户其他授权。
    /// # 参数
    /// actor、entry_permission、resource 为真实入口；targets 为完整操作目标；executor 为调用方事务。
    /// # 返回
    /// 仅对 targets 中记录有效的授权结果，单条传入一个目标。
    /// # 错误
    /// 入口失效、数据库失败或读取期间授权修订号变化时拒绝。
    pub async fn resolve_targets(
        &self,
        actor: &AuditActor,
        entry_permission: &Permission,
        resource: ResourceKind,
        targets: ResourceTargets,
        executor: &mut dyn Executor,
    ) -> Result<AuthorizedResourceAccess> {
        let query = self.resolve(actor, entry_permission, resource, executor).await?;
        let ids = resource_grants::target_ids(&self.db, actor.id(), resource, &targets, executor).await?;
        self.ensure_current(&query, executor).await?;
        Ok(AuthorizedResourceAccess { query, targets, ids })
    }

    /// 校验数据库查询前后指定记录授权未发生版本漂移。
    /// # 参数
    /// query 为本次入口解析结果；executor 为相同执行器。
    /// # 返回
    /// 当前账号资源的修订号仍与解析时一致。
    /// # 错误
    /// 版本已变化或数据库读取失败时拒绝；不代替重新校验身份及入口功能权限。
    pub async fn ensure_current(
        &self,
        query: &AuthorizedResourceQuery,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let revision = resource_grants::revision(&self.db, &query.user_id, query.resource, executor).await?;
        if revision != query.grant_revision {
            return Err(Error::ConflictError("授权范围已变化，请重新读取".into()));
        }
        Ok(())
    }
}
