//! 跨域材料查询的窄仓储读取，业务来源及授权由调用用例证明。

use erp_sales::entity::sales_order::SalesOrderRevisionLine;
use erp_sales::repository::SalesOrderExt;
use mongodb::Database;
use mongodb::bson::doc;
use persistence_core::Executor;

use crate::Result;

/// 按精确销售版本行 ID 批量读取来源事实。
/// # 参数
/// `db`、`ids` 与调用方当前 `executor`；空 ID 集合不发查询。
/// # 返回
/// 返回仍存在的版本行，顺序不承诺与输入一致。
/// # 错误
/// 数据库查询失败时传播，调用方必须独立检查缺失及归属。
pub async fn revision_lines(
    db: &Database,
    ids: &[String],
    executor: &mut dyn Executor,
) -> Result<Vec<SalesOrderRevisionLine>> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    Ok(db.sales_order_revision_lines().find_many(doc! { "id": { "$in": ids } }, executor).await?)
}
