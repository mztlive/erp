//! 发货过账前的本域加载与状态守卫，以及发货状态 CAS。
use erp_core::common::time::Instant;
use erp_core::ids::{DeliveryId, PurchaseOrderId};
use persistence_core::Executor;

use super::FulfillmentService;
use crate::entity::fulfillment::{Delivery, DeliveryLine, DeliveryState, DeliveryUpdate};
use crate::repository::FulfillmentExt;
use crate::{Error, Result};

impl FulfillmentService {
    /// 按原序加载发货单、校验草稿和版本、应用物流信息并读取非空发货行。
    ///
    /// # 错误
    /// 不存在、非草稿、版本冲突、物流更新非法或没有行时保留原首错。
    ///
    /// # 关键业务约束
    /// 此方法不读取时间和库存，不校验后续仓发行；调用方在返回后取得原过账时间。
    pub async fn prepare_delivery_posting(
        &self,
        delivery_id: &DeliveryId,
        expected_version: u64,
        update: DeliveryUpdate,
        executor: &mut dyn Executor,
    ) -> Result<(Delivery, Vec<DeliveryLine>)> {
        let mut delivery = self
            .db
            .deliveries()
            .find_by_id(delivery_id.as_ref(), executor)
            .await?
            .ok_or_else(|| Error::NotFound("发货单不存在".to_string()))?;
        if delivery.status != DeliveryState::Draft {
            return Err(Error::ConflictError("只有草稿状态的发货单可以过账".to_string()));
        }
        if delivery.base.version != expected_version {
            return Err(Error::ConflictError("发货单版本已变化，请刷新后重试".to_string()));
        }
        delivery.update(update)?;
        let lines = self
            .db
            .fulfillment()
            .delivery_lines_by_delivery_ids(std::slice::from_ref(delivery_id), executor)
            .await?;
        if lines.is_empty() {
            return Err(Error::ValidationError("发货单没有行，无法过账".to_string()));
        }
        Ok((delivery, lines))
    }

    /// 在库存或采购门槛成功之后，以同一执行器迁移发货状态并写入原版本 CAS。
    ///
    /// # 错误
    /// 状态迁移或仓储写入失败时保留原错误，调用方不得推进任务或审计。
    pub async fn persist_posted_delivery(
        &self,
        delivery: &mut Delivery,
        occurred_at: Instant,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        delivery.mark_shipped(occurred_at)?;
        self.db.deliveries().update(delivery, executor).await?;
        Ok(())
    }
}

/// 在直发分支原位置读取采购来源引用，不提前触发采购或付款校验。
///
/// # 错误
/// 供应商直发缺少采购来源时返回原业务错误。
pub fn supplier_purchase_source(delivery: &Delivery) -> Result<PurchaseOrderId> {
    delivery
        .purchase_order_id
        .clone()
        .ok_or_else(|| Error::BusinessLogicError("供应商直发缺少采购来源".to_string()))
}
