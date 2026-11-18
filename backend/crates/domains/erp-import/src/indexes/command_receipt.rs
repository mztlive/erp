//! 导入命令回执的全局唯一约束，命令身份不得软删除后复用。
use mongodb::Database;
use mongodb::bson::doc;
use persistence_core::Result;
use persistence_core::indexes::unique_index;

use crate::repository::ImportCommandReceiptExt;

// 注册独立回执的命令身份与事件关联唯一约束，供恢复时唯一定位原结果；现有数据冲突或 MongoDB 创建失败时返回错误。
pub(super) async fn ensure(db: &Database) -> Result<()> {
    let indexes = [
        ("uk_import_command_receipts_id", doc! { "id": 1 }),
        ("uk_import_command_receipts_command", doc! { "identity.command_id": 1 }),
        ("uk_import_command_receipts_audit_event", doc! { "audit_event_id": 1 }),
    ]
    .into_iter()
    .map(|(name, keys)| unique_index(name, keys));

    db.collection::<mongodb::bson::Document>(<Database as ImportCommandReceiptExt>::IMPORT_COMMAND_RECEIPTS)
        .create_indexes(indexes)
        .await?;
    Ok(())
}
