//! 供应商履约订单对象读取与写入范围；跟进人和业务组织分别解释。

use std::sync::Arc;

use application_core::AuditActor;
use mongodb::Database;
use persistence_core::Executor;

use crate::entity::supplier_fulfillment::SupplierFulfillmentOrder;
use crate::error::{Error, Result};
use crate::ports::{
    FulfillmentOrderDataScopePort, FulfillmentOrderResolvedClause, FulfillmentOrderResolvedScope,
    FulfillmentOrderScopeObject,
};
use crate::repository::{FulfillmentOrderReadScope, FulfillmentOrderScopeClause, SupplierFulfillmentExt};

/// 履约订单对象访问范围；列表、详情、候选、导出和写命令复用同一解析。
#[derive(Clone)]
pub struct FulfillmentOrderAccess {
    db: Database,
    scope: Arc<dyn FulfillmentOrderDataScopePort>,
}

impl FulfillmentOrderAccess {
    /// 绑定履约订单集合与范围授权 Port。
    ///
    /// # 参数
    /// * `db` - 订单集合所在数据库
    /// * `scope` - 组合层注入的范围 Port
    ///
    /// # 返回
    /// 返回无授权缓存的访问服务，构造不执行 I/O。
    ///
    /// # 错误
    /// 无。
    pub fn new(db: Database, scope: Arc<dyn FulfillmentOrderDataScopePort>) -> Self {
        Self { db, scope }
    }

    /// 在调用方事务内证明资源动作并映射跟进责任条件。
    ///
    /// # 参数
    /// * `actor` - 已认证操作人
    /// * `action` - 已注册动作
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 返回已解析范围事实和订单授权条件。
    ///
    /// # 错误
    /// 无动作权限返回 Forbidden；未装配时拒绝。
    pub async fn resolve(
        &self,
        actor: &AuditActor,
        action: &str,
        executor: &mut dyn Executor,
    ) -> Result<(FulfillmentOrderResolvedScope, FulfillmentOrderReadScope)> {
        let access = self.scope.resolve(actor, action, executor).await?;
        Ok((access.clone(), fulfillment_order_scope(&access, actor.id())))
    }

    /// 在调用方事务内重验订单对象资格。
    ///
    /// # 参数
    /// * `actor` - 已认证操作人
    /// * `action` - 已注册动作
    /// * `id` - 订单主键
    /// * `handler_user_id` - 当前开放 W26 处理人；无任务时为空
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 对象在范围内时返回订单。
    ///
    /// # 错误
    /// 不可见对象返回 NotFound，不泄露存在性。
    pub async fn require_order(
        &self,
        actor: &AuditActor,
        action: &str,
        id: &str,
        handler_user_id: Option<&str>,
        executor: &mut dyn Executor,
    ) -> Result<SupplierFulfillmentOrder> {
        let (access, scope) = self.resolve(actor, action, executor).await?;
        let found = self.db.supplier_fulfillment_orders().find_by_id(id, executor).await?.filter(|item| {
            in_scope(&scope, &item.follow_up_user_id, &item.business_org_unit_id)
                || handler_user_id == Some(access.user_id.as_str())
        });
        let Some(order) = found else {
            return Err(Error::NotFound("供应商履约订单不存在或无权查看".into()));
        };
        if !self.allows(&access, &order, handler_user_id)? {
            return Err(Error::NotFound("供应商履约订单不存在或无权查看".into()));
        }
        Ok(order)
    }

    /// 复用公共单对象判定检查已加载订单。
    ///
    /// # 参数
    /// * `access` - 当前动作已解析事实
    /// * `order` - 已加载订单
    /// * `handler_user_id` - 当前开放 W26 处理人
    ///
    /// # 返回
    /// 范围允许时为 true。
    ///
    /// # 错误
    /// 资源动作不符或未装配时拒绝。
    pub fn allows(
        &self,
        access: &FulfillmentOrderResolvedScope,
        order: &SupplierFulfillmentOrder,
        handler_user_id: Option<&str>,
    ) -> Result<bool> {
        self.scope.allows(
            access,
            &FulfillmentOrderScopeObject {
                owned: order.follow_up_user_id == access.user_id
                    || handler_user_id == Some(access.user_id.as_str()),
                org_unit_id: Some(order.business_org_unit_id.clone()).filter(|id| !id.is_empty()),
            },
        )
    }

    /// 展开请求组织及其可选下级，与授权用同一执行器。
    pub async fn expand_org_units(
        &self,
        org_unit_ids: &[String],
        include_descendants: bool,
        executor: &mut dyn Executor,
    ) -> Result<std::collections::BTreeSet<String>> {
        self.scope.expand_org_units(org_unit_ids, include_descendants, executor).await
    }

    /// 查询账号在解析时点的唯一主属组织。
    pub async fn own_org(
        &self,
        user_id: &str,
        at: erp_core::common::time::Instant,
        executor: &mut dyn Executor,
    ) -> Result<Option<String>> {
        self.scope.own_org(user_id, at, executor).await
    }

    /// 校验拟创建或拟写入对象落在当前动作范围内。
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
            return Err(Error::Forbidden("当前授权范围不允许维护该供应商订单".into()));
        }
        let object =
            FulfillmentOrderScopeObject { owned: owner == access.user_id, org_unit_id: Some(org.into()) };
        if !self.scope.allows(&access, &object)? {
            return Err(Error::Forbidden("当前授权范围不允许维护该供应商订单".into()));
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
/// 返回履约订单仓储条件。
///
/// # 错误
/// 无。
///
/// # 关键业务约束
/// 本人负责解释显式跟进人；组织解释业务组织。
pub fn fulfillment_order_scope(
    access: &FulfillmentOrderResolvedScope,
    user: &str,
) -> FulfillmentOrderReadScope {
    FulfillmentOrderReadScope {
        roles: access.role_clauses.iter().map(|clause| map_clause(clause, user)).collect(),
        user_limit: access.user_limit.as_ref().map(|clause| map_clause(clause, user)),
    }
}

fn map_clause(clause: &FulfillmentOrderResolvedClause, actor: &str) -> FulfillmentOrderScopeClause {
    FulfillmentOrderScopeClause {
        company: clause.company,
        owner_user_id: clause.self_owned.then(|| actor.into()),
        business_org_unit_ids: clause.org_unit_ids.clone(),
    }
}

fn in_scope(scope: &FulfillmentOrderReadScope, owner: &str, org: &str) -> bool {
    scope.allows_object(owner, org)
}

#[cfg(test)]
mod tests {
    use erp_core::common::time::Instant;

    use super::*;

    fn access(self_owned: bool, org: &[&str], company: bool) -> FulfillmentOrderResolvedScope {
        FulfillmentOrderResolvedScope {
            user_id: "actor".into(),
            resource: "supplier_fulfillment_order".into(),
            action: "list".into(),
            role_clauses: vec![FulfillmentOrderResolvedClause {
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
    fn fulfillment_scope_maps_self_owned_and_org_without_created_by() {
        let scope = fulfillment_order_scope(&access(true, &["org-a"], false), "actor");
        assert!(scope.allows_object("actor", "org-b"));
        assert!(scope.allows_object("other", "org-a"));
        assert!(!scope.allows_object("other", "org-b"));
    }

    #[test]
    fn handler_does_not_expand_owner_scope_compilation() {
        let scope = fulfillment_order_scope(&access(true, &[], false), "actor");
        assert!(scope.allows_object("actor", "org-z"));
        assert!(!scope.allows_object("handler", "org-z"));
    }
}
