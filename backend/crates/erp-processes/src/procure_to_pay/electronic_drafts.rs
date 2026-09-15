//! 采购生效后按冻结分配生成电子交付草稿与履约工作项。
use std::collections::HashMap;
use std::str::FromStr;

use erp_core::common::time::Instant;
use erp_core::ids::{PurchaseLineSalesAllocationId, PurchaseOrderId, SalesOrderLineId};
use erp_core::money::Quantity;
use erp_fulfillment::dto::CreateElectronicDeliveryRequest;
use erp_fulfillment::entity::fulfillment::{ElectronicDelivery, FulfillmentResult};
use erp_fulfillment::repository::FulfillmentExt;
use erp_fulfillment::service::electronic_delivery_crypto::electronic_delivery_draft_from_request;
use erp_procurement::entity::purchase_order::{PurchaseLineType, PurchaseOrder, PurchaseOrderRevisionLine};
use id_generator::next_id;
use persistence_core::Executor;

use crate::{Error, Result};

// 草稿占位值不含个人信息；正式确认替换为加密快照及配置密钥计算的指纹。
const DRAFT_FINGERPRINT_KEY: &[u8] = b"erp-electronic-delivery-draft-key-v1";

pub(super) async fn create_electronic_drafts(
    db: &mongodb::Database,
    order: &PurchaseOrder,
    revision_lines: &[PurchaseOrderRevisionLine],
    allocations: &HashMap<String, PurchaseLineSalesAllocationId>,
    actor_id: &str,
    executor: &mut dyn Executor,
) -> Result<()> {
    for line in revision_lines {
        if line.line_type != PurchaseLineType::ItemService {
            continue;
        }
        let Some(sales_line_id) = &line.procurement_confirmation_line_id else {
            continue;
        };
        let allocation_id = allocations
            .get(line.base.id.as_str())
            .ok_or_else(|| Error::BusinessLogicError("电子交付明细缺少销售分配，无法生成交付草稿".into()))?;
        let record = draft(
            order,
            line,
            SalesOrderLineId::new(sales_line_id.to_string()),
            allocation_id.clone(),
            actor_id,
        )?;
        db.electronic_deliveries().create(&record, executor).await?;
        crate::fulfillment_execution::task::ensure_fulfillment_task(
            db,
            crate::fulfillment_execution::task::FulfillmentTaskObject::ElectronicDelivery(&record),
            executor,
        )
        .await?;
    }
    Ok(())
}

fn draft(
    order: &PurchaseOrder,
    line: &PurchaseOrderRevisionLine,
    sales_order_line_id: SalesOrderLineId,
    allocation_id: PurchaseLineSalesAllocationId,
    actor_id: &str,
) -> Result<ElectronicDelivery> {
    let request = CreateElectronicDeliveryRequest {
        fulfillment_no: format!("ED-{}", next_id()),
        sales_order_line_id,
        purchase_order_id: PurchaseOrderId::new(order.base.id.clone()),
        purchase_line_sales_allocation_id: allocation_id,
        recipient_snapshot: "待填写".into(),
        quantity: line
            .quantity
            .unwrap_or(Quantity::from_str("0").map_err(|error| Error::Internal(error.to_string()))?),
        result: FulfillmentResult::Success,
        occurred_at: Instant::now().unix_secs(),
        evidence_attachment_id: None,
    };
    Ok(electronic_delivery_draft_from_request(request, actor_id, DRAFT_FINGERPRINT_KEY)?)
}
