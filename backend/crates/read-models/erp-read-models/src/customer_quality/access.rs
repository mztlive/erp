//! 双口径授权解析：现任口径走客户授权服务，历史口径走销售读取范围。
//!
//! 两个口径各自调用身份域公共解析入口；冻结归属快照永不提供访问资格，
//! 现任负责人回填历史贡献在此处即被阻断。

use std::sync::Arc;

use application_core::AuditActor;
use erp_customer::CustomerAccess;
use erp_identity::AuthorizationService;
use erp_permissions::internal;
use erp_sales::repository::sales_order::scope::SalesReadScope;
use mongodb::Database;
use persistence_core::Executor;

use crate::Result;
use crate::sales_center::access::{SalesAccess, SalesReadContext};

/// 双口径授权解析器；不缓存任何授权结论。
#[derive(Clone)]
pub struct QualityAccess {
    db: Database,
    rbac: Arc<AuthorizationService>,
}

impl QualityAccess {
    /// 绑定身份事实来源与组合层注入的客户授权服务。
    ///
    /// # 参数
    /// * `db` - 应用数据库
    /// * `rbac` - 当前 RBAC 快照服务
    /// * `customer_scope` - 已装配的客户授权服务
    ///
    /// # 返回
    /// 返回无授权缓存的解析器，构造不执行 I/O。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn new(db: Database, rbac: Arc<AuthorizationService>, _customer_scope: CustomerAccess) -> Self {
        Self { db, rbac }
    }

    /// 在调用方事务内证明历史口径的销售范围及独立客户列表资格。
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
    ) -> Result<(SalesReadContext, SalesReadScope)> {
        SalesAccess::new(self.db.clone(), self.rbac.clone())
            .resolve(actor, &internal::CUSTOMER_QUALITY_HISTORY, executor)
            .await
    }
}
