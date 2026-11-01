//! 独立财务回执唯一约束。

use mongodb::bson::{Document, doc};
use mongodb::options::IndexOptions;
use mongodb::{Database, IndexModel};
use persistence_core::Result;

use crate::repository::FinanceCommandExt;

/// 财务回执主 ID 唯一索引。
pub const FINANCE_COMMAND_ID_INDEX: &str = "uk_finance_command_receipts_id";
/// 创建财务回执 ID 唯一索引。
///
/// # 参数
/// * `db` - 目标数据库。
/// # 返回
/// 索引创建成功时返回空结果。
/// # 错误
/// 存量身份冲突或 MongoDB 无法创建索引时返回错误。
pub async fn ensure(db: &Database) -> Result<()> {
    db.collection::<Document>(Database::FINANCE_COMMAND_RECEIPTS).create_indexes(receipt_indexes()).await?;
    Ok(())
}

/// 回执没有 TTL 或软删除限定；其生命周期覆盖命令重试与追溯期限。
fn receipt_indexes() -> Vec<IndexModel> {
    vec![
        IndexModel::builder()
            .keys(doc! { "id": 1 })
            .options(IndexOptions::builder().name(FINANCE_COMMAND_ID_INDEX.to_string()).unique(true).build())
            .build(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unique_constraint_protects_current_command_identity_without_ttl() {
        let indexes = receipt_indexes();
        assert_eq!(indexes.len(), 1);
        for index in &indexes {
            assert_eq!(index.options.as_ref().unwrap().unique, Some(true));
        }
        assert_eq!(indexes[0].keys, doc! { "id": 1 });
        assert!(indexes.iter().all(|index| index.options.as_ref().unwrap().expire_after.is_none()));
    }
}
