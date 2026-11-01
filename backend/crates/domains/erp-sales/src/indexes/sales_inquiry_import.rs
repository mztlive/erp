//! 导入预览及确认命令身份唯一性。

use std::time::Duration;

use mongodb::bson::{Document, doc};
use mongodb::options::IndexOptions;
use mongodb::{Database, IndexModel};
use persistence_core::Result;

use crate::repository::extensions::SalesInquiryImportExt;

/// 为预览及确认回执登记稳定身份唯一索引。
/// # 参数
/// `db` 为目标数据库。
/// # 返回
/// 索引已存在或创建成功时返回空结果。
/// # 错误
/// 重复身份或 MongoDB 失败时返回存储错误。
pub async fn ensure(db: &Database) -> Result<()> {
    for collection in [Database::INQUIRY_IMPORT_PREVIEWS, Database::INQUIRY_IMPORT_RECEIPTS] {
        let index = IndexModel::builder()
            .keys(doc! {"id": 1})
            .options(IndexOptions::builder().name(format!("uk_{collection}_id")).unique(true).build())
            .build();
        db.collection::<Document>(collection).create_index(index).await?;
    }
    let ttl = IndexModel::builder()
        .keys(doc! {"expires_at_date": 1})
        .options(
            IndexOptions::builder()
                .name("ttl_sales_inquiry_import_previews".to_string())
                .expire_after(Duration::ZERO)
                .build(),
        )
        .build();
    db.collection::<Document>(Database::INQUIRY_IMPORT_PREVIEWS).create_index(ttl).await?;
    Ok(())
}
