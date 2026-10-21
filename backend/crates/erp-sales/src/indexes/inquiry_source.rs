//! 询价销售来源的唯一身份及追溯索引。
use mongodb::bson::{Document, doc};
use mongodb::options::IndexOptions;
use mongodb::{Database, IndexModel};
use persistence_core::Result;

use crate::repository::sales_order::inquiry_source::SALES_INQUIRY_SOURCES;

/// 为销售来源登记唯一销售身份及询价追溯索引。
/// # 参数
/// `db` 为目标数据库。
/// # 返回
/// 索引已存在或创建完成时成功。
/// # 错误
/// 数据冲突或数据库失败时返回错误。
pub async fn ensure(db: &Database) -> Result<()> {
    let indexes = [
        IndexModel::builder()
            .keys(doc! {"id": 1})
            .options(
                IndexOptions::builder()
                    .name("uk_sales_inquiry_sources_order".to_string())
                    .unique(true)
                    .build(),
            )
            .build(),
        IndexModel::builder()
            .keys(doc! {"inquiry_id": 1, "feedback_round": 1})
            .options(IndexOptions::builder().name("ix_sales_inquiry_sources_inquiry".to_string()).build())
            .build(),
    ];
    db.collection::<Document>(SALES_INQUIRY_SOURCES).create_indexes(indexes).await?;
    Ok(())
}
