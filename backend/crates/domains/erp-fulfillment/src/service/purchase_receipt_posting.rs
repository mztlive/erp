//! 采购入库过账的本域读取、状态守卫与事务内写回。
use std::collections::HashSet;

use erp_core::common::time::Instant;
use erp_core::ids::{DeliveryId, DeliveryLineId, PurchaseReceiptId, SalesOrderId, WarehouseId};
use id_generator::next_id;
use persistence_core::Executor;

use super::FulfillmentService;
use crate::entity::facts::ReceiptReservationLineFact;
use crate::entity::fulfillment::{
    Delivery, DeliveryData, DeliveryLineBatch, DeliveryLineSpec, DeliveryType, PurchaseReceipt,
    PurchaseReceiptLine,
};
use crate::repository::FulfillmentExt;
use crate::{Error, Result};
impl FulfillmentService {
    /// 采购收货过账前沿用调用方事务，依次读取表头、核对草稿与期望版本、核对冻结仓库，再加载并验证明细。
    /// 找不到原单时返回 `NotFound`，状态或版本失效返回 `ConflictError`，仓库不符或明细不合法返回 `ValidationError`；传播实体更新和查询错误。
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
            warehouse_id: warehouse_id.or_else(|| Some(receipt.warehouse_id.clone())),
        })?;

        let lines = self
            .db
            .fulfillment()
            .receipt_lines_by_receipt_ids(std::slice::from_ref(receipt_id), session)
            .await?;
        receipt.ensure_posting_lines(&lines).map_err(|error| Error::ValidationError(error.to_string()))?;
        Ok((receipt, lines))
    }
    /// 逐行库存写入完成后，用同一执行器登记收货单的过账状态、业务时间和操作人。
    /// 已冲正、操作人为空或超长以及仓储写入失败时返回错误。
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
    /// 入库形成销售预占后，在原事务按销售单和发货仓库查找可复用的未删除仓发草稿。无匹配返回 `None`，传播仓储错误。
    pub async fn draft_warehouse_delivery(
        &self,
        sales_order_id: &SalesOrderId,
        warehouse_id: &WarehouseId,
        session: &mut dyn Executor,
    ) -> Result<Option<Delivery>> {
        Ok(self.db.fulfillment().draft_warehouse_delivery(sales_order_id, warehouse_id, session).await?)
    }
    /// 无可复用仓发草稿时，用本次入库的销售预占创建仓发表头和明细，保持 ID、单号及明细的生成顺序。
    /// 表头和行沿调用方事务写入；单号、表头或写入失败返回原错误，明细构造失败映射为 `Logic`。
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
                tracking_entries: Vec::new(),
                address_snapshot_encrypted: None,
                address_snapshot_fingerprint: None,
            },
        )?;
        let lines = DeliveryLineBatch::build(
            delivery_id,
            DeliveryType::WarehouseShip,
            1,
            receipt_reservation_specs(reservations),
        )
        .map_err(Error::Logic)?;

        self.db.fulfillment().create_delivery_with_lines(&delivery, &lines, session).await?;
        Ok(delivery)
    }
    /// 复用仓发草稿时沿原事务读取已有关联，从最大行号之后逐行追加还未引用的入库预占。
    /// 无新增预占时不写入；传播读取及写入错误，明细构造失败映射为 `Logic`。
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
            delivery_id,
            DeliveryType::WarehouseShip,
            next_line_no,
            receipt_reservation_specs(&pending),
        )
        .map_err(Error::Logic)?
        {
            self.db.delivery_lines().create(&line, session).await?;
        }
        Ok(())
    }
}

fn receipt_reservation_specs(reservations: &[ReceiptReservationLineFact]) -> Vec<DeliveryLineSpec> {
    reservations
        .iter()
        .map(|reservation| DeliveryLineSpec {
            line_id: DeliveryLineId::new(next_id()),
            sales_order_line_id: reservation.sales_order_line_id.clone(),
            quantity: reservation.reserved_quantity,
            stock_reservation_id: Some(reservation.reservation_id.clone()),
            purchase_line_sales_allocation_id: None,
        })
        .collect()
}
