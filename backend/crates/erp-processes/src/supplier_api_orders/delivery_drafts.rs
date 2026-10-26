use std::collections::{HashMap, HashSet};

use erp_core::ids::{DeliveryId, DeliveryLineId, PurchaseLineSalesAllocationId};
use erp_fulfillment::entity::fulfillment::{DeliveryLine, DeliveryLineData, DeliveryType};
use erp_fulfillment::repository::FulfillmentExt;
use erp_procurement::entity::purchase_order::PurchaseOrder;
use erp_supply::api_order::for_purchase;
use mongodb::Database;
use persistence_core::Executor;

use crate::fulfillment_execution::task::{FulfillmentTaskObject, ensure_fulfillment_task};
use crate::procure_to_pay::review::prepare_direct_delivery;
use crate::{Error, Result};

/// 按冻结 API 子单创建独立直发草稿，避免一个包裹状态提前过账其它地址。
/// # 参数
/// `order` 为同事务正式采购；`allocations` 为同事务真实采购销售分配。
/// # 返回
/// 已由 API 草稿覆盖的采购版本行身份，调用方只为剩余手工行建通用草稿。
/// # 错误
/// 来源销售行、分配、明细或任务写入失败时回滚根事务。
pub async fn create_api_delivery_drafts(
    db: &Database,
    order: &PurchaseOrder,
    allocations: &HashMap<String, PurchaseLineSalesAllocationId>,
    executor: &mut dyn Executor,
) -> Result<HashSet<String>> {
    let executions =
        for_purchase(db, &order.base.id, order.stable.current_revision_id.as_deref(), executor).await?;
    let mut covered = HashSet::new();
    for execution in executions {
        let mut delivery = prepare_direct_delivery(db, order).await?;
        delivery.base.id = format!("{}-delivery", execution.base.id);
        let mut lines = Vec::new();
        for (index, source) in execution.source_lines.iter().enumerate() {
            let allocation = allocations
                .get(&source.purchase_order_revision_line_id)
                .ok_or_else(|| Error::BusinessLogicError("API直发缺少采购销售分配".into()))?;
            let sales = source
                .sales_order_line_id
                .as_ref()
                .ok_or_else(|| Error::BusinessLogicError("API直发缺少冻结销售行".into()))?;
            lines.push(DeliveryLine::new(
                DeliveryLineId::new(format!("{}-delivery", source.fulfillment_item_id)),
                DeliveryLineData {
                    delivery_id: DeliveryId::new(&delivery.base.id),
                    line_no: u32::try_from(index + 1)
                        .map_err(|_| Error::ValidationError("直发行数量超限".into()))?,
                    sales_order_line_id: sales.clone().into(),
                    quantity: source.quantity,
                    stock_reservation_id: None,
                    purchase_line_sales_allocation_id: Some(allocation.clone()),
                },
                DeliveryType::SupplierDirect,
            )?);
            covered.insert(source.purchase_order_revision_line_id.clone());
        }
        db.fulfillment().create_delivery_with_lines(&delivery, &lines, executor).await?;
        ensure_fulfillment_task(db, FulfillmentTaskObject::Delivery(&delivery), executor).await?;
    }
    Ok(covered)
}
