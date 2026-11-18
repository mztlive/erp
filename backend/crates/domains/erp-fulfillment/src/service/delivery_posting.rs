//! 发货过账前的本域加载与状态守卫，以及发货状态 CAS。
use erp_core::common::time::Instant;
use erp_core::ids::{DeliveryId, PurchaseOrderId};
use persistence_core::Executor;

use super::FulfillmentService;
use crate::entity::fulfillment::{Delivery, DeliveryLine, DeliveryState, DeliveryUpdate};
use crate::repository::FulfillmentExt;
use crate::{Error, Result};

impl FulfillmentService {
    /// 发货过账开始时，在调用方事务按顺序查单、核对草稿和乐观锁版本、应用物流变更，再读取非空发货行并校验物流关联。
    /// 缺单、非草稿、版本冲突、物流不合法或无发货行时按该顺序拒绝；不读取时间或库存，不提前校验后续仓发行。
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

        delivery.update(update).map_err(|error| Error::ValidationError(error.to_string()))?;

        let lines = self
            .db
            .fulfillment()
            .delivery_lines_by_delivery_ids(std::slice::from_ref(delivery_id), executor)
            .await?;
        if lines.is_empty() {
            return Err(Error::ValidationError("发货单没有行，无法过账".to_string()));
        }
        delivery.ensure_tracking_lines(&lines).map_err(|error| Error::ValidationError(error.to_string()))?;
        Ok((delivery, lines))
    }

    /// 库存或采购前置门槛通过后，用同一执行器登记发货发生时间并以原版本 CAS 写回已发货状态。
    /// 状态迁移或持久化失败时传播原错误，调用方不得继续推进任务或记录成功审计。
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

/// 只在供应商直发分支取采购单引用，不提前读取采购或付款事实。仓发或直发缺采购引用时返回 `BusinessLogicError`。
pub fn supplier_purchase_source(delivery: &Delivery) -> Result<PurchaseOrderId> {
    delivery.supplier_purchase_source().map_err(|error| Error::BusinessLogicError(error.to_string()))
}
