//! 请求去重及本人历史分页索引。
use mongodb::bson::{Document, doc};
use mongodb::options::IndexOptions;
use mongodb::{Database, IndexModel};
use persistence_core::Result;

use crate::repository::recognition::IMPORTS;

/// 为导入任务集合登记幂等命名索引。
///
/// # 参数
/// * `db` - 目标 MongoDB 数据库。
///
/// # 返回
/// 无返回值。身份、请求去重与本人历史分页索引已登记。
///
/// # 错误
/// 已有数据违反唯一约束，或 MongoDB 无法创建索引时返回错误。
pub(super) async fn ensure(db: &Database) -> Result<()> {
    let indexes = [
        ("uk_contract_imports_id", doc! { "id": 1 }, true),
        ("uk_contract_imports_request", doc! { "owner_id": 1, "command.request_key": 1 }, true),
        ("idx_contract_imports_owner", doc! { "owner_id": 1, "created_at": -1, "id": -1 }, false),
    ];
    for (name, keys, unique) in indexes {
        db.collection::<Document>(IMPORTS)
            .create_index(
                IndexModel::builder()
                    .keys(keys)
                    .options(IndexOptions::builder().name(name.to_string()).unique(unique).build())
                    .build(),
            )
            .await?;
    }
    Ok(())
}
