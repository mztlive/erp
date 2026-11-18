//! 原询价价格只按已授权销售单身份批量读取，不扫描询价或商品目录。

use erp_sales::entity::sales_order::inquiry_source::SalesOrderInquirySource;
use erp_sales::repository::sales_order::inquiry_source::inquiry_sources;
use mongodb::Database;
use mongodb::bson::doc;
use persistence_core::Executor;

use crate::Result;

// 组装销售历史价格对比时，按已授权销售单读取其保存的询价来源。
//
// 调用参数：
// `db`、已授权销售单 `ids` 与调用方的 `executor`。
//
// 返回这些销售单的已保存来源，空输入不发查询。
//
// 数据库读取或反序列化失败时返回错误。
pub(super) async fn inquiry_sources_by_orders(
    db: &Database,
    ids: &[String],
    executor: &mut dyn Executor,
) -> Result<Vec<SalesOrderInquirySource>> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    Ok(inquiry_sources(db).find_many(doc! { "id": { "$in": ids } }, executor).await?)
}
