//! 供应商交接独立命令身份唯一索引。

use mongodb::bson::{Document, doc};
use mongodb::options::IndexOptions;
use mongodb::{Database, IndexModel};
use persistence_core::Result;

use crate::repository::handover_receipt::SupplierHandoverReceiptExt;

/// 登记供应商及能力交接命令的唯一身份约束。
///
/// # 参数
/// * `db` - 目标数据库。
/// # 返回
/// 索引已存在或建立成功时成功。
/// # 错误
/// 重复命令身份或数据库错误时失败。
pub async fn ensure(db: &Database) -> Result<()> {
    let index = IndexModel::builder()
        .keys(doc! { "id": 1 })
        .options(
            IndexOptions::builder()
                .name("uk_supplier_handover_command_receipts_id".to_string())
                .unique(true)
                .build(),
        )
        .build();
    db.collection::<Document>(<Database as SupplierHandoverReceiptExt>::HANDOVER_COMMAND_RECEIPTS)
        .create_indexes([index])
        .await?;
    Ok(())
}
