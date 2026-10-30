//! 实际验收、实际入库及其冲正只唤醒原执行的核验，不在根事务访问供应商。

use erp_core::common::time::Instant;
use erp_core::ids::CustomerAcceptanceId;
use erp_fulfillment::entity::fulfillment::CustomerAcceptance;
use erp_fulfillment::repository::FulfillmentExt;
use erp_supply::api_order::{ApiOrderExecution, ApiOrderExt, for_purchase, for_sales_lines};
use mongodb::Database;
use persistence_core::Executor;

use crate::{Error, Result};

/// 在实际客户验收及反向验收根事务内唤醒原来源执行。
/// # 参数
/// 已过账原验收事实及同一事务，身份只取自其正式销售行。
/// # 返回
/// 原执行取得待核验标记；冲正也只重验，不登记肯定通知。
/// # 错误
/// 查询、范围或版本竞争失败时回滚根事务。
pub async fn wake_customer_receipts(
    db: &Database,
    acceptance: &CustomerAcceptance,
    executor: &mut dyn Executor,
) -> Result<()> {
    let lines = db
        .fulfillment()
        .acceptance_lines_by_acceptance_ids(&[CustomerAcceptanceId::new(&acceptance.base.id)], executor)
        .await?;
    let ids = lines.iter().map(|line| line.sales_order_line_id.to_string()).collect::<Vec<_>>();
    wake(db, for_sales_lines(db, &ids, executor).await?, executor).await
}

/// 在原采购实际入库或入库冲正根事务内唤醒，不把后续客户验收当作原采购收货。
/// # 参数
/// `purchase` 为实际入库反查的原采购身份及根事务。
/// # 返回
/// 原采购执行的独立收货核验被唤醒，原下单步骤不改变。
/// # 错误
/// 范围超限、查询或版本竞争失败时拒绝。
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
