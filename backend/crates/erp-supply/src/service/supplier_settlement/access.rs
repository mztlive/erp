//! 结算对象读取与写入范围；对账负责人与业务组织分别解释。

use std::sync::Arc;

use application_core::AuditActor;
use mongodb::Database;
use persistence_core::Executor;

use crate::entity::supplier_settlement::SupplierSettlementStatement;
use crate::error::{Error, Result};
use crate::ports::{
    SettlementDataScopePort, SettlementResolvedClause, SettlementResolvedScope, SettlementScopeObject,
};
use crate::repository::SupplierSettlementExt;
use crate::repository::prelude::*;
use crate::repository::supplier_settlement::{SettlementReadScope, SettlementScopeClause};

/// 结算对象访问范围；列表、详情和写命令复用同一解析。
#[derive(Clone)]
pub struct SettlementAccess {
    db: Database,
    scope: Arc<dyn SettlementDataScopePort>,
}

impl SettlementAccess {
    /// 绑定结算集合与范围授权 Port。
    ///
    /// # 参数
    /// * `db` - 结算集合所在数据库
    /// * `scope` - 组合层注入的结算范围 Port
    ///
    /// # 返回
    /// 返回无授权缓存的访问服务，构造不执行 I/O。
    pub fn new(db: Database, scope: Arc<dyn SettlementDataScopePort>) -> Self {
        Self { db, scope }
    }

    /// 在调用方事务内证明资源动作并映射结算责任条件。
    ///
    /// # 参数
    /// * `actor` - 已认证操作人
    /// * `action` - 已注册动作
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 返回已解析范围事实和结算授权条件。
    ///
    /// # 关键业务约束
    /// 责任按对账负责人和业务组织解释；创建人不成为对账负责人。
    pub async fn resolve(
        &self,
        actor: &AuditActor,
        action: &str,
        executor: &mut dyn Executor,
    ) -> Result<(SettlementResolvedScope, SettlementReadScope)> {
        let access = self.scope.resolve(actor, action, executor).await?;
        Ok((access.clone(), settlement_scope(&access, actor.id())))
    }

    /// 在调用方事务内重验结算对象资格。
    ///
    /// # 参数
    /// * `actor` - 已认证操作人
    /// * `action` - 已注册动作
    /// * `id` - 结算单主键
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 对象在范围内时返回结算单。
    ///
    /// # 错误
    /// 读取动作对不可见对象返回 NotFound；写动作返回 Forbidden。
    pub async fn require_statement(
        &self,
        actor: &AuditActor,
        action: &str,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<SupplierSettlementStatement> {
        let (access, scope) = self.resolve(actor, action, executor).await?;
        let found = self.db.supplier_settlement_statements().find_authorized(id, &scope, executor).await?;
        let Some(statement) = found else {
            return Err(deny_object(action));
        };
        if !self.allows(&access, &statement)? {
            return Err(deny_object(action));
        }
        Ok(statement)
    }

    /// 复用公共单对象判定检查已加载结算单。
    ///
    /// # 参数
    /// * `access` - 当前动作已解析事实
    /// * `statement` - 已加载结算单
    ///
    /// # 返回
    /// 范围允许时为 true。
    pub fn allows(
        &self,
        access: &SettlementResolvedScope,
        statement: &SupplierSettlementStatement,
    ) -> Result<bool> {
        self.scope.allows(
            access,
            &SettlementScopeObject {
                owned: statement.is_prepared_by(&access.user_id),
                org_unit_id: Some(statement.business_org_unit_id.clone()).filter(|id| !id.is_empty()),
            },
        )
    }

    /// 在调用方事务内证明创建资格并解析对账负责人主属组织。
    ///
    /// # 参数
    /// * `actor` - 已认证操作人
    /// * `owner_user_id` - 拟写入的对账负责人
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 创建范围覆盖拟写入责任时返回授权上下文与业务组织。
    ///
    /// # 关键业务约束
    /// 缺主属组织阻断；不得把创建人当作对账负责人兜底。
    pub async fn require_create(
        &self,
        actor: &AuditActor,
        owner_user_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<(SettlementResolvedScope, String)> {
        let access = self.scope.resolve(actor, "create", executor).await?;
        let owner_org = self
            .scope
            .own_org(owner_user_id, access.as_of, executor)
            .await?
            .ok_or_else(|| Error::ValidationError("请先维护对账负责人的有效主属组织".into()))?;
        if owner_org == "company" {
            return Err(Error::ValidationError("请先维护对账负责人的有效主属组织".into()));
        }
        let object = SettlementScopeObject {
            owned: owner_user_id == actor.id(),
            org_unit_id: Some(owner_org.clone()),
        };
        if !self.scope.allows(&access, &object)? {
            return Err(Error::Forbidden("当前账号无权在授权范围内创建结算单".into()));
        }
        Ok((access, owner_org))
    }

    /// 展开请求组织及其可选下级。
    pub async fn expand_org_units(
        &self,
        org_unit_ids: &[String],
        include_descendants: bool,
        executor: &mut dyn Executor,
    ) -> Result<std::collections::BTreeSet<String>> {
        self.scope.expand_org_units(org_unit_ids, include_descendants, executor).await
    }
}

/// 映射同角色正向范围和独立个人上限。
///
/// # 参数
/// * `access` - Port 返回的已解析授权
/// * `user` - 当前账号
///
/// # 返回
/// 返回结算仓储条件。
///
/// # 关键业务约束
/// 本人负责解释对账负责人；组织解释业务组织。
pub fn settlement_scope(access: &SettlementResolvedScope, user: &str) -> SettlementReadScope {
    SettlementReadScope {
        roles: access.role_clauses.iter().map(|clause| map_clause(clause, user)).collect(),
        user_limit: access.user_limit.as_ref().map(|clause| map_clause(clause, user)),
    }
}

/// 仅接受已解析的内部组织范围，不读取创建人作为授权。
fn map_clause(clause: &SettlementResolvedClause, actor: &str) -> SettlementScopeClause {
    SettlementScopeClause {
        company: clause.company,
        owner_user_id: clause.self_owned.then(|| actor.into()),
        business_org_unit_ids: clause.org_unit_ids.clone(),
    }
}

/// 读取动作对不可见对象隐藏存在性；写动作返回 Forbidden。
fn deny_object(action: &str) -> Error {
    if matches!(action, "list" | "detail") {
        Error::NotFound("供应商结算单不存在".into())
    } else {
        Error::Forbidden("当前账号无权操作该结算单".into())
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
    fn self_owned_maps_to_prepared_by_not_created_by() {
        let access = SettlementResolvedScope {
            user_id: "actor".into(),
            resource: "supplier_settlement_statement".into(),
            action: "list".into(),
            role_clauses: vec![SettlementResolvedClause {
                self_owned: true,
                ..SettlementResolvedClause::default()
            }],
            user_limit: None,
            policy_version: 1,
            organization_version: 1,
            scope_version: "v1".into(),
            as_of: erp_core::common::time::Instant::from_unix_secs(0),
        };
        let scope = settlement_scope(&access, "actor");
        assert_eq!(scope.roles[0].owner_user_id.as_deref(), Some("actor"));
        assert!(scope.allows_object("actor", "org-x"));
        assert!(!scope.allows_object("created-by", "org-x"));
    }
}
