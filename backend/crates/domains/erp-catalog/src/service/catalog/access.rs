//! 商品对象读取与写入范围；显式维护人和业务组织分别解释。

use std::sync::Arc;

use application_core::AuditActor;
use erp_identity::entity::authorization::{FactValue, ObjectFacts};
use erp_identity::{AccessTarget, AuthorizationService, OrganizationDirectoryService};
use erp_permissions::{Permission, catalog};
use mongodb::Database;
use persistence_core::Executor;

use crate::authorization::CompiledScope;
use crate::entity::catalog::Product;
use crate::error::{Error, Result};
use crate::repository::CatalogExt;

/// 商品对象访问范围；列表、详情、候选、导出和写命令复用同一解析。
#[derive(Clone)]
pub struct CatalogAccess {
    db: Database,
    pub(crate) authorization: Arc<AuthorizationService>,
}

impl CatalogAccess {
    /// 由组合根注入商品数据库与共享授权服务，供各用例解析对象范围。
    /// 构造不执行 I/O、不缓存授权；请求不能替换权限或查询绑定。
    pub fn new(db: Database, authorization: Arc<AuthorizationService>) -> Self {
        Self { db, authorization }
    }

    /// 在用例原事务中，为服务端固定商品动作编译完整查询凭证。
    /// 商品责任只认显式维护人和业务组织，不能用创建人补齐；无动作权限或授权服务未装配时拒绝。
    pub async fn resolve(
        &self,
        actor: &AuditActor,
        permission: &Permission,
        executor: &mut dyn Executor,
    ) -> Result<CompiledScope> {
        if permission.resource() != "product" {
            return Err(Error::ValidationError("商品访问器不能使用其他资源权限".into()));
        }
        let query = self.authorization.query(actor, permission, executor).await?;
        let compiled = Arc::new(query);
        Ok(CompiledScope {
            scope_version: compiled.version(),
            compiled,
            authorization: self.authorization.clone(),
        })
    }

    /// 需要读取或写入实际商品时，先以固定动作做完整对象授权，再交付该商品。
    /// 写命令必须在原业务事务内重验，列表凭证不能替代；无权和不存在均按 `NotFound` 处理。
    pub async fn require_product(
        &self,
        actor: &AuditActor,
        permission: &Permission,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Product> {
        self.require(actor, permission, id, executor).await?;
        self.db
            .products()
            .find_by_id(id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("对象不存在或无权访问".into()))
    }

    /// 写入既有商品前，在原业务事务中核对固定权限和对象范围，但不重复装载业务实体。
    /// 对象缺失、越权或身份设施失效时拒绝。
    pub async fn require(
        &self,
        actor: &AuditActor,
        permission: &Permission,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        if permission.resource() != "product" {
            return Err(Error::ValidationError("商品访问器不能使用其他资源权限".into()));
        }
        Ok(self.authorization.require(actor, permission, AccessTarget::record(id)?, executor).await?)
    }

    /// 商品创建事务中对即将持久化的真实候选执行固定创建政策，不能用客户端声明代替该对象事实。
    /// 账号、功能资格、范围或身份设施失效时拒绝。
    pub async fn require_create(
        &self,
        actor: &AuditActor,
        product: &Product,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        Ok(self
            .authorization
            .require(
                actor,
                &catalog::PRODUCT_PRODUCT_CREATE,
                AccessTarget::Candidate(ObjectFacts {
                    resource: "product".into(),
                    id: product.base.id.clone(),
                    owner_id: FactValue::Known(Some(product.maintainer_user_id.clone())),
                    org_unit_id: FactValue::Known(Some(product.business_org_unit_id.clone())),
                    ..Default::default()
                }),
                executor,
            )
            .await?)
    }

    /// 展开请求组织及其可选下级，与授权用同一执行器。
    ///
    /// 返回启用节点的组织 ID 集合。未知组织拒绝。
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
    /// 存在唯一主属组织时返回其 ID。同一时点多条主属关系时拒绝。
    ///
    /// 沿用调用方的执行器，不另开事务。
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
    /// 不得用创建人字段回填；组织取其主属组织，不默认公司。
    ///
    /// 返回维护人 ID 与其主属组织。维护人为空或缺少有效主属组织时拒绝。
    ///
    /// 沿用调用方的执行器，不另开事务。
    ///
    /// `requested` 为请求中的维护人；空则使用操作人。
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

    /// 写入预检或展示操作入口时检查账号的功能资格，复用原执行器。
    /// 本检查不签发对象权限；实际写入仍须对目标执行完整授权。
    pub async fn ensure_writable(
        &self,
        actor: &AuditActor,
        permission: &Permission,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        if permission.resource() != "product" {
            return Err(Error::ValidationError("商品访问器不能使用其他资源权限".into()));
        }
        if !self.authorization.has_permission(actor, permission, executor).await? {
            return Err(Error::Forbidden("没有商品写入权限".into()));
        }
        Ok(())
    }
}
