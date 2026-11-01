//! 实际条码的唯一占用索引不约束同 SKU 的历次修订。

use mongodb::bson::{Document, doc};
use mongodb::options::IndexOptions;
use mongodb::{Database, IndexModel};
use persistence_core::Result;

use crate::repository::CatalogExt;

/// 注册条码身份争用的唯一索引。
/// # 参数
/// `db` 为目标数据库。
/// # 返回
/// 占用事实的稳定主键及条码唯一约束建立。
/// # 错误
/// 已有占用冲突或数据库索引登记错误。
pub(super) async fn ensure(db: &Database) -> Result<()> {
    let indexes = [
        IndexModel::builder()
            .keys(doc! { "id": 1 })
            .options(
                IndexOptions::builder().name("uk_sku_barcode_claims_id".to_string()).unique(true).build(),
            )
            .build(),
        IndexModel::builder()
            .keys(doc! { "barcode": 1 })
            .options(
                IndexOptions::builder()
                    .name("uk_sku_barcode_claims_barcode".to_string())
                    .unique(true)
                    .build(),
            )
            .build(),
    ];
    db.collection::<Document>(<Database as CatalogExt>::SKU_BARCODE_CLAIMS).create_indexes(indexes).await?;
    Ok(())
}
