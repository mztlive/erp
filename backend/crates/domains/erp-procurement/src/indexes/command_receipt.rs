//! 采购独立命令回执唯一约束；回执无 TTL 或软删除限定。

use mongodb::bson::{Document, doc};
use mongodb::options::IndexOptions;
use mongodb::{Database, IndexModel};
use persistence_core::Result;

use crate::repository::PurchaseCommandExt;

/// 稳定命令 ID 唯一索引。
pub const PURCHASE_COMMAND_ID_INDEX: &str = "uk_purchase_command_receipts_id";

/// 注册采购独立回执索引。
///
/// # 参数
/// * `db` - 目标数据库。
/// # 返回
/// 成功时返回空结果。
/// # 错误
/// 存量命令 ID 冲突或索引写入失败时返回错误。
pub async fn ensure(db: &Database) -> Result<()> {
    db.collection::<Document>(Database::PURCHASE_COMMAND_RECEIPTS).create_indexes(receipt_indexes()).await?;
    Ok(())
}
/// 去重覆盖所有回执生命周期，不接受过期或软删除过滤。
fn receipt_indexes() -> Vec<IndexModel> {
    vec![
        IndexModel::builder()
            .keys(doc! { "id": 1 })
            .options(IndexOptions::builder().name(PURCHASE_COMMAND_ID_INDEX.to_string()).unique(true).build())
            .build(),
    ]
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn receipt_unique_index_has_no_expiration_or_partial_filter() {
        let indexes = receipt_indexes();
        assert_eq!(indexes[0].keys, doc! { "id": 1 });
        let options = indexes[0].options.as_ref().unwrap();
        assert_eq!(options.unique, Some(true));
        assert!(options.expire_after.is_none());
        assert!(options.partial_filter_expression.is_none());
    }
}
