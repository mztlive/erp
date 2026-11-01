//! 报表使用销售对象的当前读取范围，额外证明同角色成本查询权限。
use application_core::AuditActor;
use erp_identity::Permission;
use erp_identity::service::access_control::resolve::AuthorizedDataScope;
use erp_sales::repository::sales_order::scope::SalesReadScope;
use persistence_core::Executor;

use super::ActualProfitLossReadModel;
use crate::Result;
use crate::sales_center::access::SalesAccess;

impl ActualProfitLossReadModel {
    /// 在同一事务解析销售读取与成本动作权限，保留参与及个人上限规则。
    ///
    /// # 参数
    /// * `actor` - 认证用户。
    /// * `executor` - 调用方事务执行器。
    ///
    /// # 返回
    /// 返回授权数据范围及销售读取范围。
    ///
    /// # 错误
    /// `cost_entry:list` 权限解析失败，或销售范围解析失败时返回对应错误。
    pub(super) async fn authorized_scope(
        &self,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<(AuthorizedDataScope, SalesReadScope)> {
        let resolver = SalesAccess::new(self.db.clone(), self.rbac.clone());
        // 成本按来源销售分配继承范围，不存在可单独配置的成本部门范围。
        // 同角色成本查询权限由销售解析器的额外权限集合一并证明。
        resolver.resolve(actor, "list", &[Permission::parse("cost_entry:list")?], executor).await
    }
}
