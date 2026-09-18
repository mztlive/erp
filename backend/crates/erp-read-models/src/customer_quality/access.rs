//! 双口径授权解析：现任口径走客户 Port，历史口径走销售读取范围。
//!
//! 两个口径各自调用身份域公共解析入口；冻结归属快照永不提供访问资格，
//! 现任负责人回填历史贡献在此处即被阻断。

use std::sync::Arc;

use application_core::AuditActor;
use erp_customer::repository::scope::CustomerReadScope;
use erp_customer::{CustomerAccess, CustomerDataScopePort, CustomerResolvedScope};
use erp_identity::service::access_control::resolve::AuthorizedDataScope;
use erp_sales::repository::sales_order::scope::SalesReadScope;
use mongodb::Database;
use persistence_core::Executor;

use crate::Result;
use crate::sales_center::access::SalesAccess;

/// 双口径授权解析器；不缓存任何授权结论。
#[derive(Clone)]
pub struct QualityAccess {
    db: Database,
    rbac: erp_identity::SharedRbacService,
    customer_scope: Arc<dyn CustomerDataScopePort>,
}

impl QualityAccess {
    /// 绑定身份事实来源与组合层注入的客户范围 Port。
    ///
    /// # 参数
    /// * `db` - 应用数据库
    /// * `rbac` - 当前 RBAC 快照服务
    /// * `customer_scope` - 客户域 Port，生产环境由组合层 adapter 实现
    ///
    /// # 返回
    /// 返回无授权缓存的解析器，构造不执行 I/O。
    pub fn new(
        db: Database,
        rbac: erp_identity::SharedRbacService,
        customer_scope: Arc<dyn CustomerDataScopePort>,
    ) -> Self {
        Self { db, rbac, customer_scope }
    }

    /// 在调用方事务内证明现任口径的客户与销售双重动作范围。
    ///
    /// # 参数
    /// * `actor` - 已认证操作人
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 返回客户范围事实、客户授权条件与销售读取范围。
    ///
    /// # 错误
    /// 任一动作无权限、范围未注册或查询超限时拒绝。
    ///
    /// # 关键业务约束
    /// 订单行必须同时满足客户范围与销售范围；缺一保持空集。
    pub async fn resolve_current(
        &self,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<(CustomerResolvedScope, CustomerReadScope, AuthorizedDataScope, SalesReadScope)> {
        let customer = CustomerAccess::new(self.db.clone(), Arc::clone(&self.customer_scope));
        let (customer_context, customer_scope) = customer.resolve(actor, "list", executor).await?;
        let sales = SalesAccess::new(self.db.clone(), self.rbac.clone());
        let (sales_context, sales_scope) = sales.resolve(actor, "list", &[], executor).await?;
        Ok((customer_context, customer_scope, sales_context, sales_scope))
    }

    /// 在调用方事务内证明历史口径的销售读取范围。
    ///
    /// # 参数
    /// * `actor` - 已认证操作人
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 返回销售授权上下文与读取范围；冻结归属不参与授权。
    ///
    /// # 错误
    /// 无动作权限、范围未注册或查询超限时拒绝。
    pub async fn resolve_history(
        &self,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<(AuthorizedDataScope, SalesReadScope)> {
        SalesAccess::new(self.db.clone(), self.rbac.clone()).resolve(actor, "list", &[], executor).await
    }
}

/// 现任口径无范围：客户侧既无角色规则，历史参与也不补充现任客户集合。
pub(super) fn no_current_scope(customer: &CustomerResolvedScope, scope: &CustomerReadScope) -> bool {
    !customer.has_scope_rules() && scope.is_empty()
}
