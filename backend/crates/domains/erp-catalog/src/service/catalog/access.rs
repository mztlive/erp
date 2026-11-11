//! 商品对象读取与写入范围；显式维护人和业务组织分别解释。

use application_core::AuditActor;
use erp_identity::{
    AuthorizationRegistry, AuthorizationService, OrganizationDirectoryService, Permission, SharedRbacService,
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
    rbac: Option<SharedRbacService>,
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
        Self { db, rbac: Some(rbac) }
    }

    /// 构造未装配身份设施的访问器。
    /// # 参数
    /// db 为业务数据库。
    /// # 返回
    /// 所有授权入口保持失败关闭的访问器。
    /// # 错误
    /// 构造不执行 I/O。
    pub fn unwired(db: Database) -> Self {
        Self { db, rbac: None }
    }

    /// 读取已装配身份设施，缺失时拒绝授权。
    fn identity(&self) -> Result<SharedRbacService> {
        self.rbac.clone().ok_or_else(|| Error::Internal("商品身份设施未接线".into()))
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
        Ok(authorization::resolve(&self.db, self.identity()?, actor, "product", action, executor).await?)
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
        authorization::require(&self.db, self.identity()?, actor, "product", action, id, executor).await?;
        self.db
            .products()
            .find_by_id(id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("对象不存在或无权访问".into()))
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

    /// 检查共享商品写动作的功能资格。
    /// # 参数
    /// actor 为操作人，action 为写动作，executor 为原业务执行器。
    /// # 返回
    /// 有效账号具备该动作时成功。
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
        let service =
            AuthorizationService::new(self.db.clone(), self.identity()?, AuthorizationRegistry::default());
        let permission = Permission::parse(format!("product:{action}"))?;
        if !service.has_permission(actor, &permission, executor).await? {
            return Err(Error::Forbidden("没有商品写入权限".into()));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use erp_core::AccountKind;
    use persistence_core::NoTransaction;

    use super::*;

    #[tokio::test]
    async fn unwired_access_rejects_reads_and_writes_before_database_commands() {
        let db = mongodb::Client::with_options(Default::default()).unwrap().database("unused");
        let access = CatalogAccess::unwired(db);
        let actor = AuditActor::new("actor".into(), "Actor".into(), AccountKind::Admin);
        let mut executor = NoTransaction;
        assert!(matches!(access.resolve(&actor, "list", &mut executor).await, Err(Error::Internal(_))));
        assert!(matches!(
            access.require_product(&actor, "detail", "product-a", &mut executor).await,
            Err(Error::Internal(_))
        ));
        assert!(matches!(
            access.ensure_writable(&actor, "create", &mut executor).await,
            Err(Error::Internal(_))
        ));
        assert!(matches!(
            access.ensure_writable(&actor, "delete", &mut executor).await,
            Err(Error::ValidationError(_))
        ));
    }
}
