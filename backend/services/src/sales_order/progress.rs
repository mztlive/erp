//! 销售单回款/开票进度与关闭状态的派生刷新（§9.3）。

use database::{ReceivableExt, SalesOrderExt};
use entities::common::time::Instant;
use entities::ids::SalesOrderId;
use entities::sales_order::{CollectionProgress, FulfillmentProgress, InvoiceProgress};
use mongodb::Database;

use crate::errors::{Error, Result};

/// 按应收子账事实刷新销售单回款/开票进度与关闭状态。
///
/// 回款/开票进度从子账开放余额派生；关闭状态按 §9.3 判定：全部明细履约完成
/// 且客户应收结清后自动结案（开票完成不参与关闭判定）。任一字段变化时更新
/// 销售单并写版本触及；无变化时不写。
///
/// # 参数
/// * `db` - 数据库实例
/// * `session` - 事务会话执行器
/// * `sales_order_id` - 销售单
/// * `actor_id` - 审计操作人
/// * `fulfillment` - 外部已算好的履约进度；`None` 表示不修改履约进度，
///   关闭状态按销售单当前履约进度推导
///
/// # 返回
/// 无返回值。
///
/// # 错误
/// 销售单不存在或仓储写入失败时返回错误。
pub(crate) async fn update_sales_order_money_progress(
    db: &Database,
    session: &mut mongodb::ClientSession,
    sales_order_id: &SalesOrderId,
    actor_id: String,
    fulfillment: Option<FulfillmentProgress>,
) -> Result<()> {
    let mut order = db
        .sales_orders()
        .find_by_id(sales_order_id.as_ref(), session)
        .await?
        .ok_or_else(|| Error::NotFound("销售单不存在".to_string()))?;
    let accounts = db
        .receivable_accounts()
        .list_by_sales_order(sales_order_id, session)
        .await?;
    let collection = CollectionProgress::from_receivable_balances(
        accounts
            .iter()
            .map(|account| (account.open_total, account.settled_total)),
    );
    let invoice = InvoiceProgress::from_receivable_balances(
        accounts
            .iter()
            .map(|account| (account.open_invoiceable_total, account.invoiced_total)),
    );
    if !order.refresh_progress(fulfillment, collection, invoice, Instant::now(), actor_id) {
        return Ok(());
    }
    db.sales_orders().update(&mut order, session).await?;
    Ok(())
}
