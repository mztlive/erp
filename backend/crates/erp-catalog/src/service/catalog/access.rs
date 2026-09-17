//! 商品对象读取与写入范围；显式维护人和业务组织分别解释。

use std::sync::Arc;

use application_core::AuditActor;
use mongodb::Database;
use persistence_core::Executor;

use crate::entity::catalog::Product;
use crate::error::{Error, Result};
use crate::ports::{CatalogDataScopePort, CatalogResolvedClause, CatalogResolvedScope, CatalogScopeObject};
use crate::repository::{CatalogExt, CatalogReadScope, CatalogScopeClause};

/// 商品对象访问范围；列表、详情、候选、导出和写命令复用同一解析。
#[derive(Clone)]
pub struct CatalogAccess {
    db: Database,
    scope: Arc<dyn CatalogDataScopePort>,
}

impl CatalogAccess {
    /// 绑定商品集合与范围授权 Port。
    ///
    /// # 参数
    /// * `db` - 商品集合所在数据库
    /// * `scope` - 组合层注入的商品范围 Port
    ///
    /// # 返回
    /// 返回无授权缓存的访问服务，构造不执行 I/O。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// 不得在构造时补公司范围；不得直接构造身份域 Service。
    pub fn new(db: Database, scope: Arc<dyn CatalogDataScopePort>) -> Self {
        Self { db, scope }
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
    ) -> Result<(CatalogResolvedScope, CatalogReadScope)> {
        let access = self.scope.resolve(actor, action, executor).await?;
        Ok((access.clone(), catalog_scope(&access, actor.id())))
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
        let (access, scope) = self.resolve(actor, action, executor).await?;
        let found = self
            .db
            .products()
            .find_by_id(id, executor)
            .await?
            .filter(|item| in_scope(&scope, &item.maintainer_user_id, &item.business_org_unit_id));
        let Some(product) = found else {
            return Err(Error::NotFound("商品不存在或无权查看".into()));
        };
        if !self.allows(&access, &product)? {
            return Err(Error::NotFound("商品不存在或无权查看".into()));
        }
        Ok(product)
    }

    /// 复用公共单对象判定检查已加载商品。
    ///
    /// # 参数
    /// * `access` - 当前动作已解析事实
    /// * `product` - 已加载商品
    ///
    /// # 返回
    /// 范围允许时为 true。
    ///
    /// # 错误
    /// 资源动作不符或未装配时拒绝。
    pub fn allows(&self, access: &CatalogResolvedScope, product: &Product) -> Result<bool> {
        self.scope.allows(
            access,
            &CatalogScopeObject {
                owned: product.maintainer_user_id == access.user_id,
                org_unit_id: Some(product.business_org_unit_id.clone()).filter(|id| !id.is_empty()),
            },
        )
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
        self.scope.expand_org_units(org_unit_ids, include_descendants, executor).await
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
        self.scope.own_org(user_id, at, executor).await
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

    /// 校验拟创建或拟写入对象落在当前动作范围内。
    ///
    /// # 参数
    /// * `actor` - 已认证操作人
    /// * `action` - 已注册写动作
    /// * `owner` - 拟写入维护人
    /// * `org` - 拟写入业务组织
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 范围内时成功。
    ///
    /// # 错误
    /// 空范围或对象不被允许时拒绝。
    pub async fn ensure_writable(
        &self,
        actor: &AuditActor,
        action: &str,
        owner: &str,
        org: &str,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let (access, scope) = self.resolve(actor, action, executor).await?;
        if scope.is_empty() || !scope.allows_object(owner, org) {
            return Err(Error::Forbidden("当前授权范围不允许维护该商品".into()));
        }
        let object = CatalogScopeObject { owned: owner == access.user_id, org_unit_id: Some(org.into()) };
        if !self.scope.allows(&access, &object)? {
            return Err(Error::Forbidden("当前授权范围不允许维护该商品".into()));
        }
        Ok(())
    }
}

/// 映射同角色正向范围和独立个人上限，保持交集关系。
///
/// # 参数
/// * `access` - Port 返回的已解析授权
/// * `user` - 当前账号
///
/// # 返回
/// 返回商品仓储条件。
///
/// # 错误
/// 无。
///
/// # 关键业务约束
/// 本人负责解释显式维护人；组织解释业务组织。
pub fn catalog_scope(access: &CatalogResolvedScope, user: &str) -> CatalogReadScope {
    CatalogReadScope {
        roles: access.role_clauses.iter().map(|clause| map_clause(clause, user)).collect(),
        user_limit: access.user_limit.as_ref().map(|clause| map_clause(clause, user)),
    }
}

/// 仅接受已解析的内部组织范围，不读取创建人作为授权。
///
/// # 参数
/// * `clause` - Port 正向范围
/// * `actor` - 当前账号
///
/// # 返回
/// 返回商品责任条款。
///
/// # 错误
/// 无。
///
/// # 关键业务约束
/// 不得把创建人或采购负责人当作维护人。
fn map_clause(clause: &CatalogResolvedClause, actor: &str) -> CatalogScopeClause {
    CatalogScopeClause {
        company: clause.company,
        owner_user_id: clause.self_owned.then(|| actor.into()),
        business_org_unit_ids: clause.org_unit_ids.clone(),
    }
}

/// 按仓储条件精确预筛，公共判定仍由 Port 最终确认。
///
/// # 参数
/// * `scope` - 已映射的责任条件
/// * `owner` - 对象维护人
/// * `org` - 对象业务组织
///
/// # 返回
/// 预筛命中时为 true。
///
/// # 错误
/// 无。
fn in_scope(scope: &CatalogReadScope, owner: &str, org: &str) -> bool {
    scope.allows_object(owner, org)
}

#[cfg(test)]
mod tests {
    use erp_core::common::time::Instant;

    use super::*;
    use crate::ports::CatalogResolvedClause;

    fn access(self_owned: bool, org: &[&str], company: bool) -> CatalogResolvedScope {
        CatalogResolvedScope {
            user_id: "actor".into(),
            resource: "product".into(),
            action: "list".into(),
            role_clauses: vec![CatalogResolvedClause {
                company,
                self_owned,
                collaborative: false,
                org_unit_ids: org.iter().map(|id| (*id).to_string()).collect(),
            }],
            user_limit: None,
            policy_version: 1,
            organization_version: 1,
            scope_version: "v1".into(),
            as_of: Instant::from_unix_secs(0),
        }
    }

    #[test]
    fn catalog_scope_maps_self_owned_and_org_without_created_by() {
        let scope = catalog_scope(&access(true, &["org-a"], false), "actor");
        assert!(scope.allows_object("actor", "org-b"));
        assert!(scope.allows_object("other", "org-a"));
        assert!(!scope.allows_object("other", "org-b"));
    }

    #[test]
    fn catalog_scope_company_covers_any_maintainer() {
        let scope = catalog_scope(&access(false, &[], true), "actor");
        assert!(scope.is_company());
        assert!(scope.allows_object("other", "org-z"));
    }

    #[test]
    fn detail_mapping_hides_out_of_scope_maintainer() {
        let scope = catalog_scope(&access(true, &["org-a"], false), "actor");
        assert!(in_scope(&scope, "actor", "org-b"));
        assert!(in_scope(&scope, "other", "org-a"));
        assert!(!in_scope(&scope, "other", "org-b"));
        assert!(!in_scope(&catalog_scope(&access(false, &[], false), "actor"), "other", "org-a"));
    }
}
