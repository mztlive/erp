//! 导入命令回执的全局唯一约束，命令身份不得软删除后复用。
use mongodb::Database;
use mongodb::bson::doc;
use persistence_core::Result;
use persistence_core::indexes::unique_index;

use crate::repository::ImportCommandReceiptExt;

/// 登记独立回执身份和事件关联约束。
/// # 参数
/// * `db` - 当前组合根目标数据库。
/// # 返回
/// 索引建立完成返回空值。
/// # 错误
/// 唯一约束冲突或 MongoDB 操作失败。
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
