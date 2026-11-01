//! 包裹明细关联在创建、更新和发货事务中的真实来源重验。

use std::collections::HashSet;

use erp_fulfillment::entity::fulfillment::{Delivery, DeliveryLine};
use erp_procurement::repository::PurchaseOrderExt;
use erp_sales::repository::SalesOrderExt;
use mongodb::Database;
use persistence_core::Executor;

use super::purchase_context::ensure_allocation_valid;
use crate::{Error, Result};

/// 在相同执行器中核对包裹明细的销售归属和直发采购分配。
///
/// # 参数
/// * `db` - 销售、采购和履约集合所在数据库。
/// * `delivery` - 已在本事务构造或加载的发货表头。
/// * `lines` - 该发货单真实明细。
/// * `executor` - 同一授权和写入事务的执行器。
/// # 返回
/// 所有包裹明细真实归属和采购分配有效时成功。
/// # 错误
/// 包裹关联不在本次发货、销售明细缺失/跨销售或采购当前分配失效时拒绝。
pub(super) async fn ensure_tracking_sources(
    db: &Database,
    delivery: &Delivery,
    lines: &[DeliveryLine],
    executor: &mut dyn Executor,
) -> Result<()> {
    delivery.ensure_tracking_lines(lines).map_err(|error| Error::ValidationError(error.to_string()))?;
    if delivery.tracking_entries.is_empty() {
        return Ok(());
    }
    let purchase = if let Some(id) = &delivery.purchase_order_id {
        Some(
            db.purchase_orders()
                .find_by_id(id.as_ref(), executor)
                .await?
                .ok_or_else(|| Error::NotFound("来源采购单不存在".into()))?,
        )
    } else {
        None
    };
    let mut checked_lines = HashSet::new();
    for entry in &delivery.tracking_entries {
        if !checked_lines.insert(entry.sales_order_line_id.clone()) {
            continue;
        }
        let sales_line = db
            .sales_order_lines()
            .find_by_id(entry.sales_order_line_id.as_ref(), executor)
            .await?
            .ok_or_else(|| Error::ValidationError("包裹关联的销售明细不存在".into()))?;
        entry
            .ensure_sales_order(&delivery.sales_order_id, &sales_line.sales_order_id)
            .map_err(|error| Error::ValidationError(error.to_string()))?;
        if let Some(purchase) = &purchase {
            let line = lines
                .iter()
                .find(|line| line.sales_order_line_id == entry.sales_order_line_id)
                .ok_or_else(|| Error::ValidationError("包裹关联明细不在本次发货中".into()))?;
            let allocation = line
                .purchase_line_sales_allocation_id
                .as_ref()
                .ok_or_else(|| Error::ValidationError("直发包裹明细缺少采购销售分配".into()))?;
            ensure_allocation_valid(db, executor, purchase, allocation, &entry.sales_order_line_id).await?;
        }
    }
    Ok(())
}
