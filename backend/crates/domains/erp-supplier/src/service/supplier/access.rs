//! 供应商对象读取与写入范围；维护人与业务组织分别解释。

use std::sync::Arc;

use application_core::AuditActor;
use mongodb::Database;
use persistence_core::{Executor, Transactional};

use crate::error::{Error, Result};
use crate::ports::{SupplierDataScopePort, SupplierResolvedScope};
use crate::repository::scope::SupplierReadScope;

/// 供应商对象访问范围；列表、详情、导出和写命令复用同一解析。
#[derive(Clone)]
pub struct SupplierAccess {
    db: Database,
    scope: Arc<dyn SupplierDataScopePort>,
}

impl SupplierAccess {
    /// 绑定供应商事实来源与范围授权 Port。
    ///
    /// # 参数
    /// * `db` - 供应商集合所在数据库
    /// * `scope` - 组合层注入的供应商范围 Port
    ///
    /// # 返回
    /// 返回无授权缓存的访问服务，构造不执行 I/O。
    ///
    /// # 错误
    /// 无。
    pub fn new(db: Database, scope: Arc<dyn SupplierDataScopePort>) -> Self {
        Self { db, scope }
    }

    /// 在调用方事务内证明资源动作并映射供应商责任条件。
    ///
    /// # 参数
    /// * `actor` - 已认证操作人
    /// * `action` - 已注册的供应商动作
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 返回已解析范围事实和供应商授权条件。
    ///
    /// # 错误
    /// 无动作权限返回 Forbidden。
    ///
    /// # 关键业务约束
    /// 组织范围按单据业务组织解释；能力负责人不扩大可见集合。
    pub async fn resolve(
        &self,
        actor: &AuditActor,
        action: &str,
        executor: &mut dyn Executor,
    ) -> Result<(SupplierResolvedScope, SupplierReadScope)> {
        let access = self.scope.resolve(actor, action, executor).await?;
        Ok((access.clone(), access))
    }

    /// 在独立事务中重验对象资格。
    ///
    /// # 参数
    /// * `actor` - 已认证操作人
    /// * `action` - 已注册的供应商动作
    /// * `supplier_id` - 目标供应商
    ///
    /// # 返回
    /// 对象在范围内时返回授权上下文。
    ///
    /// # 错误
    /// 读取动作对不可见对象返回 NotFound；写动作返回 Forbidden。
    pub async fn require(
        &self,
        actor: &AuditActor,
        action: &str,
        supplier_id: &str,
    ) -> Result<SupplierResolvedScope> {
        let this = self.clone();
        let actor = actor.clone();
        let action = action.to_string();
        let supplier_id = supplier_id.to_string();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move { this.require_with(&actor, &action, &supplier_id, executor).await })
            })
            .await
    }

    /// 在调用方事务内重验对象资格。
    ///
    /// # 参数
    /// * `actor` - 已认证操作人
    /// * `action` - 已注册动作
    /// * `supplier_id` - 目标供应商
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 对象在范围内时返回授权上下文。
    ///
    /// # 错误
    /// 账号、动作或对象资格失效时拒绝。
    pub async fn require_with(
        &self,
        actor: &AuditActor,
        action: &str,
        supplier_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<SupplierResolvedScope> {
        self.scope.require(actor, action, supplier_id, executor).await?;
        self.scope.resolve(actor, action, executor).await
    }

    /// 在调用方事务内证明创建资格。
    ///
    /// # 参数
    /// * `actor` - 已认证操作人
    /// * `maintainer_user_id` - 拟写入的整体维护人
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 创建范围覆盖拟写入责任时返回授权上下文与业务组织。
    ///
    /// # 错误
    /// 无创建动作、范围为空或维护人没有有效主属组织时拒绝。
    ///
    /// # 关键业务约束
    /// 缺主属组织阻断；不得把创建人当作维护人兜底。
    pub async fn require_create(
        &self,
        actor: &AuditActor,
        maintainer_user_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<(SupplierResolvedScope, String)> {
        let access = self.scope.resolve(actor, "create", executor).await?;
        let owner_org = self
            .scope
            .own_org(maintainer_user_id, access.as_of, executor)
            .await?
            .ok_or_else(|| Error::ValidationError("请先维护负责人的有效主属组织".into()))?;
        Ok((access, owner_org))
    }
}

/// 返回当前阶段的身份域查询凭证。
/// # 参数
/// access 为已编译凭证，user 为兼容调用参数。
/// # 返回
/// 相同凭证。
/// # 错误
/// 不执行I/O。
pub fn supplier_scope(access: &SupplierResolvedScope, _user: &str) -> SupplierReadScope {
    access.clone()
}
