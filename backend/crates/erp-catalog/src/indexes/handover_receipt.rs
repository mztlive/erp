//! 商品交接稳定命令身份唯一约束，不依赖展示审计索引。

use mongodb::bson::{Document, doc};
use mongodb::options::IndexOptions;
use mongodb::{Database, IndexModel};
use persistence_core::Result;

use crate::repository::handover_receipt::ProductHandoverReceiptExt;

/// 登记交接回执的稳定命令唯一索引。
///
/// # 参数
/// * `db` - 目标数据库。
/// # 返回
/// 索引建立或已存在时成功。
/// # 错误
/// 重复身份或 MongoDB 索引错误时失败。
pub async fn ensure(db: &Database) -> Result<()> {
    let index = IndexModel::builder()
        .keys(doc! { "id": 1 })
        .options(
            IndexOptions::builder()
                .name("uk_product_handover_command_receipts_id".to_string())
                .unique(true)
                .build(),
        )
        .build();
    db.collection::<Document>(<Database as ProductHandoverReceiptExt>::HANDOVER_COMMAND_RECEIPTS)
        .create_indexes([index])
        .await?;
    Ok(())
}
