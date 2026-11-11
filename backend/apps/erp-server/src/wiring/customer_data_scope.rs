//! 客户固定目录与事实映射接线；规则由 identity 执行。
use std::collections::BTreeSet;
use std::sync::Arc;

use application_core::AuditActor;
use async_trait::async_trait;
use erp_core::common::time::Instant;
use erp_customer::authorization::{CustomerMapping, service};
use erp_customer::{CustomerDataScopePort, CustomerResolvedScope};
use erp_identity::entity::authorization::AccessSubject;
use erp_identity::ports::AccessMapping;
use erp_identity::{AuthorizationService, Permission, SharedRbacService};
use mongodb::Database;
use persistence_core::Executor;

use super::identity_error::map_identity_error;
use super::scope_support;
map_identity_error!(erp_customer);

#[derive(Clone)]
pub struct MongoCustomerDataScope {
    db: Database,
    service: AuthorizationService,
    current: AuthorizationService,
    mapping: Arc<dyn AccessMapping>,
}
impl MongoCustomerDataScope {
    /// 装配固定客户资源目录。
    /// # 参数
    /// db 和 rbac 为身份设施。
    /// # 返回
    /// 不缓存跨请求授权的适配器。
    /// # 错误
    /// 固定目录非法时启动失败。
    pub fn new(db: Database, rbac: SharedRbacService) -> Self {
        let mapping = CustomerMapping::shared(db.clone());
        Self {
            service: service(db.clone(), rbac.clone(), mapping.clone(), true),
            current: service(db.clone(), rbac, mapping.clone(), false),
            db,
            mapping,
        }
    }
    /// 返回客户统一授权端口。
    /// # 参数
    /// db、rbac 为共享身份设施。
    /// # 返回
    /// 共享端口。
    /// # 错误
    /// 无。
    pub fn shared(db: Database, rbac: SharedRbacService) -> Arc<dyn CustomerDataScopePort> {
        Arc::new(Self::new(db, rbac))
    }
}
#[async_trait]
impl CustomerDataScopePort for MongoCustomerDataScope {
    async fn resolve(
        &self,
        actor: &AuditActor,
        action: &str,
        executor: &mut dyn Executor,
    ) -> erp_customer::Result<CustomerResolvedScope> {
        let current = action == "current";
        let permission = Permission::parse(format!("customer:{}", if current { "detail" } else { action }))?;
        let service = if current { &self.current } else { &self.service };
        let query = service
            .query(actor, &permission, AccessSubject::Direct, executor)
            .await
            .map_err(map_identity_error)?;
        let compiled = query.compile(self.mapping.as_ref(), executor).await.map_err(map_identity_error)?;
        service.validate(&compiled, executor).await.map_err(map_identity_error)?;
        Ok(CustomerResolvedScope {
            user_id: actor.id().into(),
            current,
            scope_version: compiled.version(),
            as_of: Instant::now(),
            compiled: Some(Arc::new(compiled)),
        })
    }
    async fn validate(
        &self,
        access: &CustomerResolvedScope,
        executor: &mut dyn Executor,
    ) -> erp_customer::Result<()> {
        let compiled = access
            .compiled
            .as_ref()
            .ok_or_else(|| erp_customer::Error::Internal("客户查询缺少凭证".into()))?;
        let service = if access.current { &self.current } else { &self.service };
        service.validate(compiled, executor).await.map_err(map_identity_error)
    }

    async fn creation(
        &self,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> erp_customer::Result<CustomerResolvedScope> {
        let permission = Permission::parse("customer:create")?;
        if !self.service.has_permission(actor, &permission, executor).await.map_err(map_identity_error)? {
            return Err(erp_customer::Error::Forbidden("没有创建客户权限".into()));
        }
        let at = Instant::now();
        scope_support::own_org(&self.db, actor.id(), at, executor, Into::into, map_identity_error)
            .await?
            .ok_or_else(|| erp_customer::Error::ValidationError("请先维护负责人的有效主属组织".into()))?;
        Ok(CustomerResolvedScope {
            user_id: actor.id().into(),
            current: false,
            scope_version: String::new(),
            as_of: at,
            compiled: None,
        })
    }
    async fn expand_org_units(
        &self,
        org_unit_ids: &[String],
        include_descendants: bool,
        executor: &mut dyn Executor,
    ) -> erp_customer::Result<BTreeSet<String>> {
        scope_support::expand_org_units(
            &self.db,
            org_unit_ids,
            include_descendants,
            executor,
            Into::into,
            map_identity_error,
        )
        .await
    }

    async fn org_member_ids(
        &self,
        org_unit_ids: &BTreeSet<String>,
        at: Instant,
        executor: &mut dyn Executor,
    ) -> erp_customer::Result<Vec<String>> {
        scope_support::org_member_ids(&self.db, org_unit_ids, at, executor, Into::into).await
    }

    async fn own_org(
        &self,
        user_id: &str,
        at: Instant,
        executor: &mut dyn Executor,
    ) -> erp_customer::Result<Option<String>> {
        scope_support::own_org(&self.db, user_id, at, executor, Into::into, map_identity_error).await
    }
}

/// 构造客户访问器。
/// # 参数
/// db、rbac 为数据库与共享身份服务。
/// # 返回
/// 已装配统一授权的客户访问器。
/// # 错误
/// 无。
pub fn customer_access(db: Database, rbac: SharedRbacService) -> erp_customer::CustomerAccess {
    erp_customer::CustomerAccess::new(db.clone(), MongoCustomerDataScope::shared(db, rbac))
}
