//! 创建供给时在原事务内重验全部有效正式采购来源。

use erp_core::ids::{SalesOrderId, SalesOrderRevisionId};
use erp_procurement::entity::purchase_order::ensure_creation_sources_current;
use erp_procurement::repository::PurchaseOrderExt;
use erp_procurement::repository::prelude::*;
use erp_sales::entity::sales_order::SalesOrder;
use mongodb::Database;
use persistence_core::Executor;

use crate::{Error, Result};

/// 在创建写入前检查来源；不按当前销售行过滤原有采购。
/// # 参数
/// `db` 是组合数据库，`order` 是本事务读取的销售正式头，`executor` 是原事务执行器。
/// # 返回
/// 所有有效正式采购均采用当前销售版本时返回 `Ok(())`。
/// # 错误
/// 缺少当前销售版本、旧采购尚未正式变更或仓储读取失败时拒绝。
pub(super) async fn ensure_sources_current(
    db: &Database,
    order: &SalesOrder,
    executor: &mut dyn Executor,
) -> Result<()> {
    let current =
        order.current_revision_id().ok_or_else(|| Error::ConflictError("来源销售单缺少正式版本".into()))?;
    let purchases = db
        .purchase_orders()
        .find_covering_by_sales_order(&SalesOrderId::new(&order.base.id), executor)
        .await?;
    ensure_creation_sources_current(&SalesOrderRevisionId::new(current), &purchases)
        .map_err(|error| Error::ConflictError(error.to_string()))
}
