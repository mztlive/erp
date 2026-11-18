//! 命令完成后的结果读取重新建立授权快照，不复用已结束事务的检查。
use application_core::AuditActor;
use erp_permissions::{Permission, sales_order as sales_order_permissions};
use erp_read_models::sales_center::order::dto::SalesOrderDetailView;
use erp_sales::dto::sales_order::WorkingCopyView;
use erp_sales::entity::sales_order::SalesOrderWorkingCopy;
use erp_sales::repository::SalesOrderExt;
use erp_sales::service::sales_order::SalesOrderService;
use persistence_core::Transactional;

use super::super::SalesOrderCommandProcess;
use crate::{Error, Result};

impl SalesOrderCommandProcess {
    /// 在新的快照内按原命令入口重新授权并读取完整结果。
    /// # 参数
    /// id、actor 为已完成命令的真实对象和账号；permission 为命令固定的结果读取资格。
    /// # 返回
    /// 与新授权快照一致的完整销售结果，不追加详情菜单资格。
    /// # 错误
    /// 当前资格、归属、有效期或读取失败时丢弃结果。
    pub(in crate::order_to_cash) async fn command_result(
        &self,
        id: &str,
        actor: &AuditActor,
        permission: &Permission,
    ) -> Result<SalesOrderDetailView> {
        let reads = self.read_model();
        let id = id.to_string();
        let actor = actor.clone();
        let permission = permission.clone();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move { Ok(reads.command_detail(&id, &actor, &permission, executor).await?) })
            })
            .await
    }

    /// 在同一新快照核验当前修改资格、工作副本身份并装配行视图。
    /// # 参数
    /// copy 为刚保存或重开的服务端副本，actor 为原命令操作人。
    /// # 返回
    /// 当前持久化副本及其同快照行视图。
    /// # 错误
    /// 无修改资格、副本缺失、归属不符或读取失败时拒绝。
    pub(super) async fn working_copy_result(
        &self,
        copy: &SalesOrderWorkingCopy,
        actor: &AuditActor,
    ) -> Result<WorkingCopyView> {
        let access = self.command_access(actor, sales_order_permissions::HANDOVER);
        let db = self.db.clone();
        let order_id = copy.sales_order_id.clone();
        let copy_id = copy.base.id.clone();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    access.current(order_id.as_ref(), executor).await?;
                    let copy = db
                        .sales_order_working_copies()
                        .find_by_id(&copy_id, executor)
                        .await?
                        .ok_or_else(|| Error::NotFound("工作副本不存在".into()))?;
                    if copy.sales_order_id != order_id {
                        return Err(Error::ConflictError("工作副本归属已变化".into()));
                    }

                    let view = SalesOrderService::new(db).working_copy_view(&copy, executor).await?;
                    access.current(order_id.as_ref(), executor).await?;
                    Ok(view)
                })
            })
            .await
    }
}
