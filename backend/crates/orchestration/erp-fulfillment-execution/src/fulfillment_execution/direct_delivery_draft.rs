//! 采购和供应商执行共同使用的直发草稿准备步骤。

use erp_core::ids::PurchaseOrderId;
use erp_fulfillment::entity::fulfillment::{Delivery, DeliveryData, DeliveryId, DeliveryType};
use erp_fulfillment::service::document_number::next_delivery_no;
use erp_procurement::entity::purchase_order::PurchaseOrder;
use id_generator::next_id;
use mongodb::Database;

use crate::{Error, Result};

/// 在原调用位置分配发货标识和单号；行验证仍在此步骤之后执行。
///
/// # 参数
/// * `db` - 履约单号所在数据库。
/// * `order` - 已由消费方校验的采购来源。
///
/// # 返回
/// 返回尚未保存明细的直发草稿。
///
/// # 错误
/// 单号分配或领域构造失败时返回原错误。
pub async fn prepare_direct_delivery(db: &Database, order: &PurchaseOrder) -> Result<Delivery> {
    let delivery_id = DeliveryId::new(next_id());
    Delivery::new(
        delivery_id.clone(),
        DeliveryData {
            delivery_no: next_delivery_no(db).await?,
            delivery_type: DeliveryType::SupplierDirect,
            sales_order_id: order.sales_order_id.clone(),
            purchase_order_id: Some(PurchaseOrderId::new(order.base.id.clone())),
            warehouse_id: None,
            tracking_entries: Vec::new(),
            address_snapshot_encrypted: None,
            address_snapshot_fingerprint: None,
        },
    )
    .map_err(Error::Logic)
}
