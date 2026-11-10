//! 业务授权唯一入口；查询和对象检查共享目录、规则与事实版本。
mod registry;
mod require;
mod resolve;
mod validate;

use std::slice;
use std::sync::Arc;

use application_core::AuditActor;
use mongodb::Database;
use persistence_core::Executor;
pub use registry::{AccessPolicy, AccessRegistration, AuthorizationRegistry};

use crate::entity::authorization::AccessSubject;
use crate::{AuthorizedQuery, Error, Permission, QualifiedPermissions, Result, SharedRbacService};

/// 授权服务。组合根装配不可变目录，业务只提供操作与目标。
#[derive(Clone)]
pub struct AuthorizationService {
    db: Database,
    rbac: SharedRbacService,
    registry: Arc<AuthorizationRegistry>,
    token: Arc<()>,
}

impl AuthorizationService {
    /// 查询单个功能资格；不能作为对象访问或写入凭证。
    /// # 参数
    /// actor 为当前账号，permission 为入口权限，executor 为原执行器。
    /// # 返回
    /// 账号有效且至少一个启用角色授予权限时为 true，否则为 false。
    /// # 错误
    /// 通配入口、数据库或策略读取失败返回错误，不按无权处理。
    pub async fn has_permission(
        &self,
        actor: &AuditActor,
        permission: &Permission,
        executor: &mut dyn Executor,
    ) -> Result<bool> {
        Ok(self.qualification(actor, slice::from_ref(permission), executor).await?.is_some())
    }

    /// 批量读取功能资格，不生成对象授权凭证。
    /// # 参数
    /// actor、permissions 为真实账号与具体权限集合，executor 为原执行器。
    /// # 返回
    /// 返回逐项独立授权结果；账号失效时为 None。
    /// # 错误
    /// 身份设施、策略版本或非法权限错误透传。
    pub async fn permissions(
        &self,
        actor: &AuditActor,
        permissions: &[Permission],
        executor: &mut dyn Executor,
    ) -> Result<Option<QualifiedPermissions>> {
        self.rbac.qualified_permissions(actor, permissions, executor).await
    }

    /// 绑定身份设施和服务端登记。
    /// # 参数
    /// db、rbac 为身份设施；registry 为已经校验的完整目录。
    /// # 返回
    /// 不启动事务的授权服务。
    /// # 错误
    /// 不执行 I/O。
    pub fn new(db: Database, rbac: SharedRbacService, registry: AuthorizationRegistry) -> Self {
        Self { db, rbac, registry: Arc::new(registry), token: Arc::new(()) }
    }

    /// 证明入口资格并生成完整查询约束。
    /// # 参数
    /// actor、permission 为服务端身份与入口；subject 为登记模型；executor 为原执行器。
    /// # 返回
    /// 不可伪造的查询约束，不装载全量授权 ID。
    /// # 错误
    /// 账号、权限、登记或事实取证失败时拒绝。
    pub async fn query(
        &self,
        actor: &AuditActor,
        permission: &Permission,
        subject: AccessSubject,
        executor: &mut dyn Executor,
    ) -> Result<AuthorizedQuery> {
        let entry = self.registry.entry(permission, &subject)?;
        let mapping =
            entry.mapping.clone().ok_or_else(|| Error::ValidationError("本操作未接入查询 Mapping".into()))?;
        let model = mapping.model().clone();
        let proof = self.resolve(actor, entry, executor).await?;
        Ok(AuthorizedQuery { proof, mapping, model })
    }
}
