//! 采购依据更新记录索引：同一采购只允许一条开放记录。
use mongodb::bson::{Document, doc};
use mongodb::options::IndexOptions;
use mongodb::{Collection, Database, IndexModel};
use persistence_core::Result;

use crate::entity::purchase_order::PurchaseSourceUpdateStatus;
use crate::repository::purchase_source_update::PURCHASE_SOURCE_UPDATES;

/// 注册采购依据更新记录索引。
///
/// # 参数
/// * `db` - 目标 MongoDB 数据库
///
/// # 返回
/// 全部索引注册成功时无返回值。
///
/// # 错误
/// 存量数据违反唯一约束或索引操作失败时返回错误。
pub(super) async fn ensure(db: &Database) -> Result<()> {
    let collection = db.collection::<Document>(PURCHASE_SOURCE_UPDATES);
    let unique = |name: &str| IndexOptions::builder().name(name.to_string()).unique(true).build();
    let named = |name: &str| IndexOptions::builder().name(name.to_string()).build();
    let open_only = IndexOptions::builder()
        .name("uk_purchase_source_update_open".to_string())
        .unique(true)
        .partial_filter_expression(doc! { "status": PurchaseSourceUpdateStatus::Open.as_str() })
        .build();
    create(&collection, doc! { "id": 1 }, unique("uk_purchase_source_update_id")).await?;
    create(&collection, doc! { "purchase_order_id": 1 }, open_only).await?;
    create(
        &collection,
        doc! { "purchase_order_id": 1, "created_at": -1, "id": -1 },
        named("ix_purchase_source_update_purchase"),
    )
    .await?;
    create(
        &collection,
        doc! { "sales_order_id": 1, "created_at": 1, "id": 1 },
        named("ix_purchase_source_update_sales"),
    )
    .await?;
    create(
        &collection,
        doc! { "trigger_sales_change_order_id": 1, "created_at": 1, "id": 1 },
        named("ix_purchase_source_update_trigger"),
    )
    .await
}

/// 创建单个索引。
async fn create(collection: &Collection<Document>, keys: Document, options: IndexOptions) -> Result<()> {
    collection.create_index(IndexModel::builder().keys(keys).options(options).build()).await?;
    Ok(())
}
