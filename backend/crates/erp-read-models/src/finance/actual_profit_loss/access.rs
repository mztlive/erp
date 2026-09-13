//! 报表使用销售对象的当前读取范围，额外证明同角色成本查询权限。
use super::ActualProfitLossReadModel;
use crate::{sales_center::access::SalesAccess, Result};
use application_core::AuditActor;
use erp_identity::service::access_control::resolve::AuthorizedDataScope;
use erp_identity::Permission;
use erp_sales::repository::sales_order::scope::SalesReadScope;
use persistence_core::Executor;

impl ActualProfitLossReadModel {
    /// 在同一事务解析销售读取与成本动作权限，保留参与及个人上限规则。
    pub(super) async fn authorized_scope(
        &self,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<(AuthorizedDataScope, SalesReadScope)> {
        let resolver = SalesAccess::new(self.db.clone(), self.rbac.clone());
        let (mut context, mut scope) = resolver
            .resolve(actor, "list", &[Permission::parse("cost_entry:list")?], executor)
            .await?;
        let (cost_context, cost_scope) = resolver
            .resolve_resource(
                actor,
                "cost_entry",
                "list",
                &[Permission::parse("sales_order:list")?],
                executor,
            )
            .await?;
        context.scope_version = format!("{}:{}", context.scope_version, cost_context.scope_version);
        scope.required_scopes.push(cost_scope);
        Ok((context, scope))
    }
}
