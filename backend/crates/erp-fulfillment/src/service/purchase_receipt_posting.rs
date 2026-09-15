//! 采购入库过账的本域读取、状态守卫与事务内写回。
use std::collections::HashSet;

use erp_core::common::time::Instant;
use erp_core::ids::{DeliveryId, PurchaseReceiptId, SalesOrderId, WarehouseId};
use id_generator::next_id;
use persistence_core::Executor;

use super::FulfillmentService;
use crate::entity::facts::ReceiptReservationLineFact;
use crate::entity::fulfillment::{
    Delivery, DeliveryData, DeliveryLineBatch, DeliveryType, PurchaseReceipt, PurchaseReceiptLine,
};
use crate::repository::FulfillmentExt;
use crate::{Error, Result};
impl FulfillmentService {
    /// 在调用方事务读取草稿、按原顺序校验状态/版本/冻结仓库，再读取并校验行。
    pub async fn prepare_purchase_receipt_posting(
        &self,
        receipt_id: &PurchaseReceiptId,
        expected_version: u64,
        warehouse_id: Option<WarehouseId>,
        session: &mut dyn Executor,
    ) -> Result<(PurchaseReceipt, Vec<PurchaseReceiptLine>)> {
        let mut receipt = self
            .db
            .purchase_receipts()
            .find_by_id(receipt_id.as_ref(), session)
            .await?
            .ok_or_else(|| Error::NotFound("采购入库单不存在".to_string()))?;
        receipt
            .ensure_draft_version(expected_version)
            .map_err(|error| Error::ConflictError(error.to_string()))?;
        if warehouse_id.as_ref().is_some_and(|requested| requested != &receipt.warehouse_id) {
            return Err(Error::ValidationError("采购入库单的目标仓库已冻结，不能在过账时变更".to_string()));
        }
        receipt.update(crate::entity::fulfillment::PurchaseReceiptUpdate {
            warehouse_id: warehouse_id.or(Some(receipt.warehouse_id.clone())),
        })?;
        let lines = self
            .db
            .fulfillment()
            .receipt_lines_by_receipt_ids(std::slice::from_ref(receipt_id), session)
            .await?;
        receipt.ensure_posting_lines(&lines).map_err(|error| Error::ValidationError(error.to_string()))?;
        Ok((receipt, lines))
    }
    /// 在逐行库存写入后标记已过账，并以同一执行器持久化状态。
    pub async fn mark_purchase_receipt_posted(
        &self,
        receipt: &mut PurchaseReceipt,
        occurred_at: Instant,
        actor_id: &str,
        session: &mut dyn Executor,
    ) -> Result<()> {
        receipt.mark_posted(occurred_at, actor_id.to_string())?;
        self.db.purchase_receipts().update(receipt, session).await?;
        Ok(())
    }
    /// 查询同销售单、同仓库可复用的仓发草稿。
    pub async fn draft_warehouse_delivery(
        &self,
        sales_order_id: &SalesOrderId,
        warehouse_id: &WarehouseId,
        session: &mut dyn Executor,
    ) -> Result<Option<Delivery>> {
        Ok(self.db.fulfillment().draft_warehouse_delivery(sales_order_id, warehouse_id, session).await?)
    }
    /// 按原身份/编号生成顺序创建入库预占对应的仓发草稿及其行。
    pub async fn create_receipt_stock_delivery(
        &self,
        sales_order_id: &SalesOrderId,
        warehouse_id: &WarehouseId,
        reservations: &[ReceiptReservationLineFact],
        session: &mut dyn Executor,
    ) -> Result<Delivery> {
        let delivery_id = DeliveryId::new(next_id());
        let delivery = Delivery::new(
            delivery_id.clone(),
            DeliveryData {
                delivery_no: super::document_number::next_delivery_no(&self.db).await?,
                delivery_type: DeliveryType::WarehouseShip,
                sales_order_id: sales_order_id.clone(),
                purchase_order_id: None,
                warehouse_id: Some(warehouse_id.clone()),
                carrier: None,
                tracking_no: None,
                address_snapshot_encrypted: None,
                address_snapshot_fingerprint: None,
            },
        )?;
        let lines = DeliveryLineBatch::build(
            delivery_id.clone(),
            DeliveryType::WarehouseShip,
            1,
            super::delivery_lines::receipt_reservation_specs(reservations),
        )
        .map_err(Error::Logic)?;
        self.db.fulfillment().create_delivery_with_lines(&delivery, &lines, session).await?;
        Ok(delivery)
    }
    /// 向既有仓发草稿追加尚未引用的采购入库预占。
    pub async fn append_receipt_stock_delivery_lines(
        &self,
        delivery: &Delivery,
        reservations: &[ReceiptReservationLineFact],
        session: &mut dyn Executor,
    ) -> Result<()> {
        let delivery_id = DeliveryId::new(delivery.base.id.clone());
        let existing = self
            .db
            .fulfillment()
            .delivery_lines_by_delivery_ids(std::slice::from_ref(&delivery_id), session)
            .await?;
        let existing_reservations = existing
            .iter()
            .filter_map(|line| line.stock_reservation_id.as_ref().map(ToString::to_string))
            .collect::<HashSet<_>>();
        let pending = reservations
            .iter()
            .filter(|reservation| !existing_reservations.contains(reservation.reservation_id.as_ref()))
            .cloned()
            .collect::<Vec<_>>();
        let next_line_no = existing.iter().map(|line| line.line_no).max().unwrap_or(0) + 1;
        for line in DeliveryLineBatch::build(
            delivery_id.clone(),
            DeliveryType::WarehouseShip,
            next_line_no,
            super::delivery_lines::receipt_reservation_specs(&pending),
        )
        .map_err(Error::Logic)?
        {
            self.db.delivery_lines().create(&line, session).await?;
        }
        Ok(())
    }
}
