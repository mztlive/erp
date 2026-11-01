use std::collections::BTreeMap;

use erp_procurement::entity::purchase_order::{PurchaseLineType, PurchaseOrder, PurchaseOrderRevisionLine};
use erp_supply::api_order::{ApiOrderExecution, ApiOrderExt, ApiOrderSourceLine};
use erp_supply::entity::supplier_offering::OfferingSourceType;
use erp_supply::repository::SupplierOfferingExt;
use mongodb::Database;
use persistence_core::Executor;
use sha2::{Digest, Sha256};

use crate::{Error, Result};

/// 在采购正式化根事务中写入 API 执行意图，不发送外部请求。
/// # 参数
/// `order`、`lines` 为同事务新建的正式采购事实；`executor` 为调用方唯一执行器。
/// # 返回
/// 手工供给不入队；API 行按连接、收件资料及预期交期确定分组。
/// # 错误
/// 精确供给、连接或冻结数量缺失及数据库失败时返回错误。
pub async fn enqueue_purchase(
    db: &Database,
    order: &PurchaseOrder,
    lines: &[PurchaseOrderRevisionLine],
    executor: &mut dyn Executor,
) -> Result<()> {
    let mut groups: BTreeMap<(String, String), Vec<ApiOrderSourceLine>> = BTreeMap::new();
    for line in lines.iter().filter(|line| line.line_type == PurchaseLineType::ItemService) {
        let Some(source) = &line.supplier_offering_source else {
            continue;
        };
        let offering = db
            .supplier_offerings()
            .find_by_id(source.supplier_offering_id.as_ref(), executor)
            .await?
            .ok_or_else(|| Error::NotFound("采购冻结供给不存在".into()))?;
        if offering.source_type != OfferingSourceType::Api {
            continue;
        }
        let connection = offering
            .source_connection_id
            .as_ref()
            .ok_or_else(|| Error::BusinessLogicError("API 供给缺少连接".into()))?;
        let allocations = allocations_for(line)?;
        for (index, quantity, key) in allocations {
            let rows =
                groups.entry((connection.to_string(), format!("{}\0{key}", line.base.id))).or_default();
            rows.push(ApiOrderSourceLine {
                fulfillment_item_id: String::new(),
                purchase_order_revision_line_id: line.base.id.clone(),
                sales_order_line_id: line.sales_order_line_id.as_ref().map(ToString::to_string),
                supplier_offering_revision_id: source.supplier_offering_revision_id.to_string(),
                quantity,
                delivery_allocation_index: index,
            });
        }
    }
    persist_groups(db, order, groups, executor).await
}

async fn persist_groups(
    db: &Database,
    order: &PurchaseOrder,
    groups: BTreeMap<(String, String), Vec<ApiOrderSourceLine>>,
    executor: &mut dyn Executor,
) -> Result<()> {
    let revision = order
        .stable
        .current_revision_id
        .as_ref()
        .ok_or_else(|| Error::BusinessLogicError("采购尚未生效".into()))?;
    for ((connection, key), mut rows) in groups {
        let id = format!(
            "api-{}",
            hex::encode(Sha256::digest(format!("{}\0{}\0{}", revision, connection, key)))
                .chars()
                .take(40)
                .collect::<String>()
        );
        for (index, row) in rows.iter_mut().enumerate() {
            row.fulfillment_item_id = format!("{id}-{}", index + 1);
        }
        if db.supplier_api_order_executions().find_by_id(&id, executor).await?.is_some() {
            continue;
        }
        let mut intent = ApiOrderExecution::queued(
            id,
            order.base.id.clone(),
            revision.clone(),
            connection,
            order.supplier_id.to_string(),
            rows,
        );
        if let Some(warehouse_id) = &order.target_warehouse_id {
            use erp_warehouse::WarehouseExt;
            intent.warehouse_revision_id = db
                .warehouses()
                .find_by_id(warehouse_id.as_ref(), executor)
                .await?
                .and_then(|warehouse| warehouse.stable.current_revision_id);
        }
        db.supplier_api_order_executions().create(&intent, executor).await?;
    }
    Ok(())
}

fn allocations_for(
    line: &PurchaseOrderRevisionLine,
) -> Result<Vec<(u32, erp_core::money::Quantity, String)>> {
    let quantity = line.quantity.ok_or_else(|| Error::BusinessLogicError("采购数量缺失".into()))?;
    if quantity.to_decimal().is_sign_negative() {
        return Err(Error::ValidationError("采购数量不能为负".into()));
    }
    if quantity.to_decimal().is_zero() {
        return Ok(Vec::new());
    }
    if line.delivery_allocations.is_empty() {
        return Ok(vec![(u32::MAX, quantity, "warehouse-address".into())]);
    }
    let total =
        line.delivery_allocations.iter().try_fold(rust_decimal::Decimal::ZERO, |sum, allocation| {
            sum.checked_add(allocation.quantity.to_decimal())
                .ok_or_else(|| Error::ValidationError("采购地址数量超限".into()))
        })?;
    if total != quantity.to_decimal() {
        return Err(Error::BusinessLogicError("采购冻结地址数量合计与采购行不一致".into()));
    }
    line.delivery_allocations
        .iter()
        .enumerate()
        .map(|(index, allocation)| {
            let key = serde_json::to_string(&(
                allocation.recipient.as_str(),
                allocation.phone.as_str(),
                allocation.region_code.as_str(),
                allocation.address.as_str(),
                line.expected_delivery_date,
            ))
            .map_err(|_| Error::Internal("采购地址分组失败".into()))?;
            Ok((
                u32::try_from(index).map_err(|_| Error::ValidationError("地址数量超限".into()))?,
                allocation.quantity,
                key,
            ))
        })
        .collect()
}
