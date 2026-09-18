//! 供应商对象读取与写入范围；维护人与业务组织分别解释。

use std::sync::Arc;

use application_core::AuditActor;
use mongodb::Database;
use persistence_core::{Executor, Transactional};

use crate::error::{Error, Result};
use crate::ports::{
    SupplierDataScopePort, SupplierResolvedClause, SupplierResolvedScope, SupplierScopeObject,
};
use crate::repository::SupplierExt;
use crate::repository::prelude::*;
use crate::repository::scope::{SupplierReadScope, SupplierScopeClause};

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
        Ok((access.clone(), supplier_scope(&access, actor.id())))
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
        let (access, scope) = self.resolve(actor, action, executor).await?;
        let found = self.db.supplier_accounts().find_authorized(supplier_id, &scope, executor).await?;
        let Some(account) = found else {
            return Err(deny_object(action));
        };
        let object = SupplierScopeObject {
            owned: account.maintainer_user_id == access.user_id,
            historical_read_participant: false,
            org_unit_id: Some(account.business_org_unit_id.clone()).filter(|value| !value.is_empty()),
        };
        if !self.scope.allows(&access, &object)? {
            return Err(deny_object(action));
        }
        Ok(access)
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
        let object = SupplierScopeObject {
            owned: maintainer_user_id == actor.id(),
            org_unit_id: Some(owner_org.clone()),
            ..Default::default()
        };
        if !self.scope.allows(&access, &object)? {
            return Err(Error::Forbidden("当前账号无权在授权范围内创建供应商".into()));
        }
        Ok((access, owner_org))
    }
}

/// 映射同角色正向范围和独立个人上限。
///
/// # 参数
/// * `access` - Port 返回的已解析授权
/// * `user` - 当前账号
///
/// # 返回
/// 返回供应商仓储条件。
///
/// # 错误
/// 无。
///
/// # 关键业务约束
/// 本人负责解释整体维护人；组织解释业务组织。
pub fn supplier_scope(access: &SupplierResolvedScope, user: &str) -> SupplierReadScope {
    SupplierReadScope {
        roles: access.role_clauses.iter().map(|clause| map_clause(clause, user)).collect(),
        user_limit: access.user_limit.as_ref().map(|clause| map_clause(clause, user)),
    }
}

/// 仅接受已解析的内部组织范围，不读取创建人作为授权。
fn map_clause(clause: &SupplierResolvedClause, actor: &str) -> SupplierScopeClause {
    SupplierScopeClause {
        company: clause.company,
        owner_user_id: clause.self_owned.then(|| actor.into()),
        business_org_unit_ids: clause.org_unit_ids.clone(),
    }
}

/// 读取动作对不可见对象隐藏存在性；写动作返回 Forbidden。
fn deny_object(action: &str) -> Error {
    if matches!(action, "list" | "detail") {
        Error::NotFound("供应商不存在".into())
    } else {
        Error::Forbidden("当前账号无权操作该供应商".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_commands_deny_out_of_scope_as_forbidden() {
        match deny_object("update") {
            Error::Forbidden(_) => {},
            other => panic!("expected forbidden, got {other:?}"),
        }
        match deny_object("detail") {
            Error::NotFound(_) => {},
            other => panic!("expected not found, got {other:?}"),
        }
    }

    #[test]
    fn self_owned_maps_to_maintainer_not_created_by() {
        let access = SupplierResolvedScope {
            user_id: "actor".into(),
            resource: "supplier".into(),
            action: "list".into(),
            role_clauses: vec![SupplierResolvedClause {
                self_owned: true,
                ..SupplierResolvedClause::default()
            }],
            user_limit: None,
            policy_version: 1,
            organization_version: 1,
            scope_version: "v1".into(),
            as_of: erp_core::common::time::Instant::from_unix_secs(0),
        };
        let scope = supplier_scope(&access, "actor");
        assert_eq!(scope.roles[0].owner_user_id.as_deref(), Some("actor"));
        assert!(scope.allows_object("actor", "org-x"));
        assert!(!scope.allows_object("created-by", "org-x"));
    }
}
