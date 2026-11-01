//! 询价、来源反查与独立命令回执索引。
use mongodb::bson::{Document, doc};
use mongodb::options::IndexOptions;
use mongodb::{Database, IndexModel};
use persistence_core::Result;

use crate::repository::SalesInquiryExt;

/// 注册询价身份、责任列表、客户、来源查询及回执唯一索引。
/// # 参数
/// `db` 为目标数据库。
/// # 返回
/// 索引已存在或创建完成时返回空结果。
/// # 错误
/// 存量重复身份或数据库创建失败时返回错误。
pub async fn ensure(db: &Database) -> Result<()> {
    let indexes = [
        (doc! { "id": 1 }, "uk_sales_inquiries_id", true),
        (doc! { "inquiry_no": 1 }, "uk_sales_inquiries_no", true),
        (
            doc! { "sales_owner_user_id": 1, "deleted_at": 1, "created_at": -1, "id": -1 },
            "idx_sales_inquiries_owner",
            false,
        ),
        (
            doc! { "business_org_unit_id": 1, "deleted_at": 1, "created_at": -1, "id": -1 },
            "idx_sales_inquiries_org",
            false,
        ),
        (
            doc! { "customer_id": 1, "deleted_at": 1, "created_at": -1, "id": -1 },
            "idx_sales_inquiries_customer",
            false,
        ),
        (
            doc! { "fee_ledger.source_document_id": 1, "deleted_at": 1, "id": 1 },
            "idx_sales_inquiries_fee_source",
            false,
        ),
        (
            doc! { "quantity_ledger.sales_order_id": 1, "deleted_at": 1 },
            "idx_sales_inquiries_sales_source",
            false,
        ),
    ];
    for (keys, name, unique) in indexes {
        register(db, Database::SALES_INQUIRIES, keys, name, unique).await?;
    }
    register(
        db,
        Database::SALES_INQUIRY_COMMAND_RECEIPTS,
        doc! { "id": 1 },
        "uk_sales_inquiry_receipts_id",
        true,
    )
    .await?;
    register(
        db,
        Database::SALES_INQUIRY_COMMAND_RECEIPTS,
        doc! { "actor_id": 1, "action": 1, "idempotency_key_hash": 1 },
        "uk_sales_inquiry_receipts_command",
        true,
    )
    .await
}

async fn register(db: &Database, collection: &str, keys: Document, name: &str, unique: bool) -> Result<()> {
    let options = IndexOptions::builder().name(name.to_string()).unique(unique).build();
    db.collection::<Document>(collection)
        .create_index(IndexModel::builder().keys(keys).options(options).build())
        .await?;
    Ok(())
}
