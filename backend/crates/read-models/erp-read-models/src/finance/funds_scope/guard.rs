//! 资金命令范围守卫。

use application_core::AuditActor;
use erp_finance::repository::ReceivableExt;
use erp_identity::Permission;
use erp_identity::access_control::ScopedObject;
use erp_identity::entity::resource_access::ResourceKind;
use erp_identity::entity::resource_access::query::ResourceTargets;
use erp_identity::service::resource_access::ResourceAccessService;
use erp_sales::repository::{SalesOrderExt, SalesOrderRepositoryScopeExt};
use persistence_core::Executor;

use super::authorization::*;
use crate::{Error, Result};

impl FundsAccess {
    /// 应收子账命令守卫：详情动作重验关联销售当前负责人与登记经办。
    ///
    /// # 参数
    /// * `actor` - 已认证操作人。
    /// * `id` - 应收往来子账主键。
    /// * `executor` - 调用方执行器。
    ///
    /// # 返回
    /// 详情动作允许该子账时成功，无返回值。
    ///
    /// # 错误
    /// 授权解析失败时返回对应错误；子账或来源销售单不存在，或当前责任不允许时返回 `NotFound`。
    pub(super) async fn guard_receivable_account(
        &self,
        actor: &AuditActor,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let (_access, authorization) = self.resolve(actor, "receivable_account", "detail", executor).await?;
        let account = self
            .db
            .receivable_accounts()
            .find_by_id(id, executor)
            .await
            .map_err(Error::from)?
            .ok_or_else(|| Error::NotFound("应收往来子账不存在".into()))?;
        let order = self
            .db
            .sales_orders()
            .find_by_id(account.sales_order_id.as_ref(), executor)
            .await
            .map_err(Error::from)?
            .ok_or_else(|| Error::NotFound("应收往来子账不存在".into()))?;
        if self
            .db
            .sales_orders()
            .find_authorized(&order.base.id, &authorization.sales, executor)
            .await?
            .is_none()
        {
            return Err(Error::NotFound("应收往来子账不存在".into()));
        }
        Ok(())
    }
}

impl FundsAccess {
    /// 在原事务重验财务入口和全部真实销售来源。
    /// # 参数
    /// actor 为当前身份；permission 为财务入口；ids 为服务端完整来源；executor 为原事务。
    /// # 返回
    /// 每一个存在的来源均有读取资格时成功。
    /// # 错误
    /// 空来源、缺失来源、功能失效或任一来源无权时拒绝。
    pub async fn require_sales_sources(
        &self,
        actor: &AuditActor,
        permission: &str,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<()> {
        if ids.is_empty() {
            return Err(Error::Forbidden("操作缺少真实销售来源".into()));
        }
        let service = ResourceAccessService::new(self.db.clone(), self.rbac.clone());
        let entry = Permission::parse(permission)?;
        for batch in ids.chunks(500) {
            let targets = ResourceTargets::new(&batch.iter().map(String::as_str).collect::<Vec<_>>())?;
            let access =
                service.resolve_targets(actor, &entry, ResourceKind::SalesOrder, targets, executor).await?;
            let orders = self.db.sales_orders().list_active_by_ids(batch, executor).await?;
            for id in batch {
                let order = orders
                    .iter()
                    .find(|order| &order.base.id == id)
                    .ok_or_else(|| Error::NotFound("销售来源不存在或不可访问".into()))?;
                if !access.allows(
                    id,
                    &ScopedObject {
                        owned: order.sales_owner_user_id == actor.id(),
                        collaborating: false,
                        historical_read_participant: false,
                        org_unit_id: Some(&order.business_org_unit_id),
                        settlement_party_id: None,
                        warehouse_id: None,
                    },
                ) {
                    return Err(Error::Forbidden("操作包含不可访问的销售来源".into()));
                }
            }
        }
        Ok(())
    }
}
