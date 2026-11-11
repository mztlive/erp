//! 商品对象读取与写入范围；显式维护人和业务组织分别解释。

use std::sync::Arc;

use application_core::AuditActor;
use erp_identity::{
    AccessMapping, AccessSubject, AccessTarget, AuthorizationService, OrganizationDirectoryService,
    Permission, ResourceTargets, SharedRbacService,
};
use mongodb::Database;
use persistence_core::Executor;

use crate::authorization::{self, CompiledScope};
use crate::entity::catalog::Product;
use crate::error::{Error, Result};
use crate::repository::CatalogExt;

/// 商品对象访问范围；列表、详情、候选、导出和写命令复用同一解析。
#[derive(Clone)]
pub struct CatalogAccess {
    db: Database,
    rbac: SharedRbacService,
    authorization: AuthorizationService,
    mapping: Arc<dyn AccessMapping>,
}

impl CatalogAccess {
    /// 绑定商品集合与共享身份设施。
    ///
    /// # 参数
    /// * `db` - 商品集合所在数据库
    /// * `rbac` - 组合层注入的共享身份设施
    ///
    /// # 返回
    /// 返回无授权缓存的访问服务，构造不执行 I/O。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// 目录由本域固定登记，不能由业务请求替换。
    pub fn new(db: Database, rbac: SharedRbacService) -> Self {
        let (authorization, mapping) =
            authorization::service(&db, rbac.clone()).expect("固定商品授权目录有效");
        Self { db, rbac, authorization, mapping }
    }

    /// 在调用方事务内证明资源动作并映射商品责任条件。
    ///
    /// # 参数
    /// * `actor` - 已认证操作人
    /// * `action` - 已注册动作
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 返回已解析范围事实和商品授权条件。
    ///
    /// # 错误
    /// 无动作权限返回 Forbidden；未装配时拒绝。
    ///
    /// # 关键业务约束
    /// 责任按显式维护人和业务组织解释；创建人不成为维护人。
    pub async fn resolve(
        &self,
        actor: &AuditActor,
        action: &str,
        executor: &mut dyn Executor,
    ) -> Result<CompiledScope> {
        let permission = Permission::parse(format!("product:{action}"))?;
        let query = self.authorization.query(actor, &permission, AccessSubject::Direct, executor).await?;
        let compiled = Arc::new(query.compile(self.mapping.as_ref(), executor).await?);
        self.authorization.validate(&compiled, executor).await?;
        Ok(CompiledScope {
            scope_version: compiled.version(),
            compiled,
            authorization: self.authorization.clone(),
        })
    }

    /// 在调用方事务内重验商品对象资格。
    ///
    /// # 参数
    /// * `actor` - 已认证操作人
    /// * `action` - 已注册动作
    /// * `id` - 商品主键
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 对象在范围内时返回商品。
    ///
    /// # 错误
    /// 不可见对象返回 NotFound，不泄露存在性。
    ///
    /// # 关键业务约束
    /// 写命令必须在原领域事务内调用；列表授权不替代对象重验。
    pub async fn require_product(
        &self,
        actor: &AuditActor,
        action: &str,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Product> {
        self.require(actor, action, id, executor).await?;
        self.db
            .products()
            .find_by_id(id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("对象不存在或无权访问".into()))
    }

    /// 在原业务事务内检查已有商品的完整授权。
    /// # 参数
    /// actor 为当前账号，action/id 为实际动作与商品，executor 为原事务。
    /// # 返回
    /// 对象授权通过时成功，不再重复装载业务实体。
    /// # 错误
    /// 对象缺失、无权或身份设施失效时拒绝。
    pub async fn require(
        &self,
        actor: &AuditActor,
        action: &str,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        Ok(self
            .authorization
            .require(
                actor,
                &Permission::parse(format!("product:{action}"))?,
                AccessTarget::Records { subject: AccessSubject::Direct, ids: ResourceTargets::new(&[id])? },
                executor,
            )
            .await?)
    }

    /// 在创建事务内检查拟写入商品的完整授权。
    /// # 参数
    /// actor 为当前账号，id 为拟创建商品主键，executor 为原写事务。
    /// # 返回
    /// 固定创建政策允许本次目标时成功。
    /// # 错误
    /// 账号、功能资格、范围或身份设施失效时拒绝。
    pub async fn require_create(
        &self,
        actor: &AuditActor,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let registry = authorization::creation_registry(id)?;
        Ok(AuthorizationService::new(self.db.clone(), self.rbac.clone(), registry)
            .require(
                actor,
                &Permission::parse("product:create")?,
                AccessTarget::Records { subject: AccessSubject::Direct, ids: ResourceTargets::new(&[id])? },
                executor,
            )
            .await?)
    }

    /// 展开请求组织及其可选下级，与授权用同一执行器。
    ///
    /// # 参数
    /// * `org_unit_ids` - 请求中的组织 ID
    /// * `include_descendants` - 是否包含有效下级
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 返回启用节点的组织 ID 集合。
    ///
    /// # 错误
    /// 未知组织拒绝。
    pub async fn expand_org_units(
        &self,
        org_unit_ids: &[String],
        include_descendants: bool,
        executor: &mut dyn Executor,
    ) -> Result<std::collections::BTreeSet<String>> {
        Ok(OrganizationDirectoryService::new(self.db.clone())
            .expand_org_units(org_unit_ids, include_descendants, executor)
            .await?)
    }

    /// 查询账号在解析时点的唯一主属组织。
    ///
    /// # 参数
    /// * `user_id` - 账号
    /// * `at` - 授权时点
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 存在唯一主属组织时返回其 ID。
    ///
    /// # 错误
    /// 同一时点多条主属关系时拒绝。
    pub async fn own_org(
        &self,
        user_id: &str,
        at: erp_core::common::time::Instant,
        executor: &mut dyn Executor,
    ) -> Result<Option<String>> {
        Ok(OrganizationDirectoryService::new(self.db.clone()).own_org(user_id, at, executor).await?)
    }

    /// 解析拟写入的维护人及其主属组织，缺组织阻断。
    ///
    /// # 参数
    /// * `requested` - 请求中的维护人；空则使用操作人
    /// * `actor` - 已认证操作人
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 返回维护人 ID 与其主属组织。
    ///
    /// # 错误
    /// 维护人为空或缺少有效主属组织时拒绝。
    ///
    /// # 关键业务约束
    /// 不得用创建人字段回填；组织取其主属组织，不默认公司。
    pub async fn maintainer_org(
        &self,
        requested: Option<&str>,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<(String, String)> {
        let maintainer = requested.map(str::trim).filter(|value| !value.is_empty()).unwrap_or(actor.id());
        if maintainer.is_empty() {
            return Err(Error::ValidationError("商品维护人不能为空".into()));
        }
        let as_of = erp_core::common::time::Instant::now();
        let org = self.own_org(maintainer, as_of, executor).await?.ok_or_else(|| {
            Error::BusinessLogicError("维护人缺少有效主属组织，请先维护组织成员关系".into())
        })?;
        Ok((maintainer.to_string(), org))
    }

    /// 检查商品写入按钮和预检入口的功能资格。
    /// # 参数
    /// actor 为操作人，action 为写动作，executor 为原业务执行器。
    /// # 返回
    /// 有效账号具备该动作时成功；实际写入仍须执行对象授权。
    /// # 错误
    /// 缺少功能资格或身份取证失败时拒绝。
    pub async fn ensure_writable(
        &self,
        actor: &AuditActor,
        action: &str,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        if !matches!(action, "create" | "update") {
            return Err(Error::ValidationError("未登记商品写动作".into()));
        }
        let permission = Permission::parse(format!("product:{action}"))?;
        if !self.authorization.has_permission(actor, &permission, executor).await? {
            return Err(Error::Forbidden("没有商品写入权限".into()));
        }
        Ok(())
    }
}
