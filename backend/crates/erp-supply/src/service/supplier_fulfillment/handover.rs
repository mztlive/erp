//! 供应商履约订单跟进人交接：范围重验、CAS 与责任字段。

use application_core::AuditActor;
use validator::Validate;

use super::SupplierFulfillmentService;
use crate::dto::supplier_fulfillment::{HandoverFulfillmentOrderRequest, HandoverFulfillmentOrderView};
use crate::entity::supplier_fulfillment::SupplierFulfillmentOrder;
use crate::error::{Error, Result};
use crate::repository::SupplierFulfillmentExt;

impl SupplierFulfillmentService {
    /// 在调用方事务内交接跟进人；供组合层与幂等收据共用执行器。
    ///
    /// # 参数
    /// * `id` - 订单稳定 ID
    /// * `req` - 已校验交接请求
    /// * `handler_user_id` - 当前开放 W26 处理人
    /// * `actor` - 已认证操作人
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 返回交接后视图。
    ///
    /// # 错误
    /// 版本、范围或目标非法时拒绝。
    pub async fn apply_fulfillment_handover(
        &self,
        id: &str,
        req: &HandoverFulfillmentOrderRequest,
        handler_user_id: Option<&str>,
        actor: &AuditActor,
        executor: &mut dyn persistence_core::Executor,
    ) -> Result<HandoverFulfillmentOrderView> {
        req.validate()?;
        persist_handover(&self.db, &self.access(), id, req, handler_user_id, actor, executor).await
    }
}

/// 在调用方事务内重验范围、CAS 并写入跟进人。
async fn persist_handover(
    db: &mongodb::Database,
    access: &crate::service::supplier_fulfillment::FulfillmentOrderAccess,
    id: &str,
    req: &HandoverFulfillmentOrderRequest,
    handler_user_id: Option<&str>,
    actor: &AuditActor,
    executor: &mut dyn persistence_core::Executor,
) -> Result<HandoverFulfillmentOrderView> {
    let mut order = access.require_order(actor, "handover", id, handler_user_id, executor).await?;
    if order.base.version != req.expected_version {
        return Err(Error::ConflictError("供应商履约订单责任或版本已变化，请刷新后重试".into()));
    }
    order.handover(req.target_user_id.clone(), req.target_org_unit_id.clone()).map_err(Error::from)?;
    access
        .ensure_writable(actor, "handover", &order.follow_up_user_id, &order.business_org_unit_id, executor)
        .await?;
    db.supplier_fulfillment_orders().update(&mut order, executor).await?;
    Ok(handover_view(&order, Vec::new()))
}

/// 构造交接响应。
pub(crate) fn handover_view(
    order: &SupplierFulfillmentOrder,
    transferred_work_item_ids: Vec<String>,
) -> HandoverFulfillmentOrderView {
    HandoverFulfillmentOrderView {
        order_id: order.base.id.clone(),
        follow_up_user_id: order.follow_up_user_id.clone(),
        business_org_unit_id: order.business_org_unit_id.clone(),
        version: order.base.version,
        transferred_work_item_ids,
    }
}
