//! 地址包裹保存、部分发货和剩余任务共用原发货事务。
use std::sync::Arc;

use erp_core::ids::DeliveryId;
use erp_fulfillment::entity::fulfillment::{
    Delivery, DeliveryAddressSnapshot, DeliveryData, DeliveryLine, DeliveryPacking,
};
use erp_fulfillment::repository::FulfillmentExt;
use erp_fulfillment::service::document_number::next_delivery_no;
use erp_identity::AuthorizationService;
use erp_party::SensitiveDataCodec;
use id_generator::next_id;
use mongodb::Database;
use persistence_core::Executor;

use super::delivery_addresses::{address_source, external_delivery, freeze_addresses, guard_sources};
use super::task::{FulfillmentTaskObject, ensure_fulfillment_task};
use crate::{Error, Result};

pub(super) struct PackingContext {
    pub codec: std::sync::Arc<SensitiveDataCodec>,
    pub key: Vec<u8>,
    pub input: Option<DeliveryPacking>,
    pub legacy_tracking: bool,
}

impl PackingContext {
    /// 校验并冻结完整地址包裹草稿；旧请求不得覆盖已升级草稿。
    /// # 参数
    /// 数据库、真实发货和行、本次输入、是否确认及同一事务执行器。
    /// # 返回
    /// 已应用草稿的可信地址快照；旧格式无输入时返回空。
    /// # 错误
    /// 地址不明、来源变化、数量或包裹不合法时拒绝。
    pub(super) async fn apply(
        &self,
        db: &Database,
        delivery: &mut Delivery,
        lines: &[DeliveryLine],
        posting: bool,
        executor: &mut dyn Executor,
    ) -> Result<Option<DeliveryAddressSnapshot>> {
        if (delivery.packing.is_some() || self.input.is_some()) && self.legacy_tracking {
            return Err(Error::ConflictError("请刷新后使用地址包裹登记".into()));
        }
        let packing = self.input.clone().or_else(|| delivery.packing.clone());
        let Some(packing) = packing else {
            if posting && !external_delivery(db, delivery, executor).await? {
                return Err(Error::ValidationError("请按收货地址登记包裹和本次发货数量后再确认".into()));
            }
            return Ok(None);
        };
        guard_sources(db, delivery, executor).await?;
        let source = address_source(db, &self.codec, &self.key, delivery, lines, executor)
            .await?
            .ok_or_else(|| Error::ValidationError("缺少可核对的收货分配，请先补充来源交付安排".into()))?;
        packing.validate(&source, lines, posting)?;
        delivery.tracking_entries = packing.tracking(lines)?;
        delivery.packing = Some(packing);
        freeze_addresses(&self.codec, delivery, &source)?;
        Ok(Some(source))
    }

    /// 在原发货事务中核对来源并拆出本次行，后续库存及来源状态校验由原流程执行，付款不阻止确认。
    /// # 参数
    /// 数据库、任务能力、草稿、真实行及同一执行器。
    /// # 返回
    /// 返回本次需要过账的真实行。
    /// # 错误
    /// 来源、包裹、数量或剩余任务失败时回滚。
    pub(super) async fn prepare_posting(
        &self,
        db: &Database,
        identity: &Arc<AuthorizationService>,
        delivery: &mut Delivery,
        lines: Vec<DeliveryLine>,
        executor: &mut dyn Executor,
    ) -> Result<Vec<DeliveryLine>> {
        let source = self.apply(db, delivery, &lines, true, executor).await?;
        super::delivery_tracking::ensure_tracking_sources(db, delivery, &lines, executor).await?;
        match source {
            Some(source) => {
                guard_sources(db, delivery, executor).await?;
                self.partition(db, identity, delivery, &lines, source, executor).await
            },
            None => Ok(lines),
        }
    }

    /// 将本次发货拆成逐地址正式行，未发数量在同一事务生成新任务。
    /// # 参数
    /// 数据库、任务能力、发货草稿、真实行、校验后的收件分配及调用方执行器。
    /// # 返回
    /// 原发货单保留本次数量；返回其按地址拆开的行。
    /// # 错误
    /// 数量不守恒、保存行或创建剩余任务失败时回滚整个事务。
    pub(super) async fn partition(
        &self,
        db: &Database,
        identity: &Arc<AuthorizationService>,
        delivery: &mut Delivery,
        original: &[DeliveryLine],
        source: DeliveryAddressSnapshot,
        executor: &mut dyn Executor,
    ) -> Result<Vec<DeliveryLine>> {
        let packing = delivery.packing.clone().ok_or_else(|| Error::Internal("发货包裹缺失".into()))?;
        let mut rest = new_remainder(delivery, next_id(), next_delivery_no(db).await?)?;
        let (shipped, remaining, current_source, rest_source, current_packing) =
            packing.partition(delivery, &rest, original, &source, next_id)?;
        for line in original {
            db.delivery_lines().soft_delete(&mut line.clone(), executor).await?;
        }
        for line in &shipped {
            db.delivery_lines().create(line, executor).await?;
        }
        delivery.packing = Some(current_packing);
        delivery.tracking_entries = delivery.packing.as_ref().expect("本次包裹已赋值").tracking(&shipped)?;
        freeze_addresses(&self.codec, delivery, &current_source)?;
        if !remaining.is_empty() {
            freeze_addresses(&self.codec, &mut rest, &rest_source)?;
            db.fulfillment().create_delivery_with_lines(&rest, &remaining, executor).await?;
            ensure_fulfillment_task(db, identity, FulfillmentTaskObject::Delivery(&rest), executor).await?;
        }
        Ok(shipped)
    }
}

fn new_remainder(source: &Delivery, id: String, number: String) -> Result<Delivery> {
    Ok(Delivery::new(
        DeliveryId::new(id),
        DeliveryData {
            delivery_no: number,
            delivery_type: source.delivery_type,
            sales_order_id: source.sales_order_id.clone(),
            purchase_order_id: source.purchase_order_id.clone(),
            warehouse_id: source.warehouse_id.clone(),
            tracking_entries: Vec::new(),
            address_snapshot_encrypted: None,
            address_snapshot_fingerprint: None,
        },
    )?)
}
