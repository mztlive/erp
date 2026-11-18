//! 唤醒原执行的收货核验；业务调用方在同一事务提供已过账来源身份。

use erp_core::common::time::Instant;
use mongodb::Database;
use persistence_core::Executor;

use super::{ApiOrderExecution, ApiOrderExt, for_purchase, for_sales_lines};
use crate::{Error, Result};

/// 按正式销售行唤醒既有供应商执行的独立收货核验。
/// 原执行取得待核验标记，不发送请求或记录肯定通知。
/// 查询、数量超限或版本竞争失败时拒绝根事务。
/// `sales_line_ids` 由已过账验收或冲正的原明细取得；`executor` 为根事务执行器。
pub async fn wake_sales_line_receipts(
    db: &Database,
    sales_line_ids: &[String],
    executor: &mut dyn Executor,
) -> Result<()> {
    wake(db, for_sales_lines(db, sales_line_ids, executor).await?, executor).await
}

/// 按实际入库或冲正的原采购身份唤醒既有执行核验。
/// 仅唤醒原采购的独立收货核验，不改变原下单步骤。
/// 查询、数量超限或版本竞争失败时拒绝根事务。
/// `purchase` 为原采购，`executor` 为根事务执行器。
pub async fn wake_purchase_receipts(
    db: &Database,
    purchase: &str,
    executor: &mut dyn Executor,
) -> Result<()> {
    wake(db, for_purchase(db, purchase, None, executor).await?, executor).await
}

async fn wake(db: &Database, rows: Vec<ApiOrderExecution>, executor: &mut dyn Executor) -> Result<()> {
    if rows.len() > 1000 {
        return Err(Error::BusinessLogicError("实际收货关联执行过多，请分批处理原业务单据".into()));
    }
    for mut execution in rows {
        execution.wake_receipt_confirmation(Instant::now());
        db.supplier_api_order_executions().update(&mut execution, executor).await?;
    }
    Ok(())
}
