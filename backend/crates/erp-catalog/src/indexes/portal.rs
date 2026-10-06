//! Proposal identity and bounded supplier queues.

use mongodb::bson::{Document, doc};
use mongodb::options::IndexOptions;
use mongodb::{Database, IndexModel};
use persistence_core::Result;

use crate::portal::CatalogPortalExt;

/// Register the catalog-owned proposal indexes.
/// # 参数
/// `db` 为目标数据库。
/// # 返回
/// 索引登记成功。
/// # 错误
/// 重复稳定身份或数据库错误。
pub async fn ensure(db: &Database) -> Result<()> {
    let indexes = [
        IndexModel::builder()
            .keys(doc! { "id": 1 })
            .options(
                IndexOptions::builder()
                    .name("uk_supplier_new_product_drafts_id".to_string())
                    .unique(true)
                    .build(),
            )
            .build(),
        IndexModel::builder()
            .keys(doc! { "supplier_id": 1, "status": 1, "created_at": -1, "id": 1 })
            .options(
                IndexOptions::builder().name("idx_supplier_new_product_drafts_queue".to_string()).build(),
            )
            .build(),
        IndexModel::builder()
            .keys(doc! { "task_id": 1 })
            .options(IndexOptions::builder().name("idx_supplier_new_product_drafts_task".to_string()).build())
            .build(),
    ];
    db.collection::<Document>(<Database as CatalogPortalExt>::NEW_PRODUCT_DRAFTS)
        .create_indexes(indexes)
        .await?;
    ensure_category_mappings(db).await?;
    Ok(())
}

/// 分类映射按供应商、原始完整路径和商品类型保持独立稳定身份。
async fn ensure_category_mappings(db: &Database) -> Result<()> {
    let indexes = [
        IndexModel::builder()
            .keys(doc! { "id": 1 })
            .options(
                IndexOptions::builder()
                    .name("uk_supplier_category_mappings_id".to_string())
                    .unique(true)
                    .build(),
            )
            .build(),
        IndexModel::builder()
            .keys(doc! { "supplier_id": 1, "original_category_path": 1, "product_kind": 1 })
            .options(
                IndexOptions::builder()
                    .name("uk_supplier_category_mappings_source".to_string())
                    .unique(true)
                    .build(),
            )
            .build(),
    ];
    db.collection::<Document>(<Database as CatalogPortalExt>::SUPPLIER_CATEGORY_MAPPINGS)
        .create_indexes(indexes)
        .await?;
    Ok(())
}
