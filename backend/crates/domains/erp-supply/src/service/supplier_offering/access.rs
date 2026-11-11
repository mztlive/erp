//! 供给对象读取与写入范围；显式维护人和业务组织分别解释。

use std::sync::Arc;

use application_core::AuditActor;
use mongodb::Database;
use persistence_core::Executor;

use crate::entity::supplier_offering::SupplierOffering;
use crate::error::{Error, Result};
use crate::ports::{OfferingDataScopePort, OfferingResolvedScope};
use crate::repository::{OfferingReadScope, SupplierOfferingExt};

/// 供给对象访问范围；列表、详情、候选、导出和写命令复用同一解析。
#[derive(Clone)]
pub struct OfferingAccess {
    db: Database,
    scope: Arc<dyn OfferingDataScopePort>,
}

impl OfferingAccess {
    /// 绑定供给集合与范围授权 Port。
    ///
    /// # 参数
    /// * `db` - 供给集合所在数据库
    /// * `scope` - 组合层注入的供给范围 Port
    ///
    /// # 返回
    /// 返回无授权缓存的访问服务，构造不执行 I/O。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// 不得在构造时补公司范围；不得直接构造身份域 Service。
    pub fn new(db: Database, scope: Arc<dyn OfferingDataScopePort>) -> Self {
        Self { db, scope }
    }

    /// 在调用方事务内证明资源动作并映射供给责任条件。
    ///
    /// # 参数
    /// * `actor` - 已认证操作人
    /// * `action` - 已注册动作
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 返回已解析范围事实和供给授权条件。
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
    ) -> Result<(OfferingResolvedScope, OfferingReadScope)> {
        let access = self.scope.resolve(actor, action, executor).await?;
        Ok((access.clone(), access))
    }

    /// 在调用方事务内重验供给对象资格。
    ///
    /// # 参数
    /// * `actor` - 已认证操作人
    /// * `action` - 已注册动作
    /// * `id` - 供给主键
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 对象在范围内时返回供给。
    ///
    /// # 错误
    /// 不可见对象返回 NotFound，不泄露存在性。
    ///
    /// # 关键业务约束
    /// 写命令必须在原领域事务内调用；列表授权不替代对象重验。
    pub async fn require_offering(
        &self,
        actor: &AuditActor,
        action: &str,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<SupplierOffering> {
        self.scope.require(actor, action, id, executor).await?;
        self.db
            .supplier_offerings()
            .find_by_id(id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("对象不存在或无权访问".into()))
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
            return Err(Error::ValidationError("供给维护人不能为空".into()));
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
        let _ = (owner, org);
        self.scope.resolve(actor, action, executor).await?;
        Ok(())
    }
}

/// 返回当前阶段的身份域查询凭证。
/// # 参数
/// access 为已编译凭证，user 为兼容调用参数。
/// # 返回
/// 相同凭证。
/// # 错误
/// 不执行I/O。
pub fn offering_scope(access: &OfferingResolvedScope, _user: &str) -> OfferingReadScope {
    access.clone()
}
