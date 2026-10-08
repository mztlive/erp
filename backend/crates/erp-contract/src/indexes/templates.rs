//! 新增集合唯一身份、领号重放和分页索引。

use mongodb::bson::{Document, doc};
use mongodb::options::IndexOptions;
use mongodb::{Database, IndexModel};
use persistence_core::Result;

use crate::repository::templates::{APPLICATIONS, COMPANY_NUMBERING, COUNTERS, TEMPLATES};

/// 为模板、主体编号、年度流水和领号申请集合登记幂等命名索引。
///
/// # 参数
/// * `db` - 目标 MongoDB 数据库。
///
/// # 返回
/// 无返回值。各集合的身份唯一索引，以及模板目录、申请去重与本人分页索引已登记。
///
/// # 错误
/// 已有数据违反唯一约束，或 MongoDB 无法创建索引时返回错误。
pub(super) async fn ensure(db: &Database) -> Result<()> {
    for collection in [TEMPLATES, COMPANY_NUMBERING, COUNTERS, APPLICATIONS] {
        db.collection::<Document>(collection)
            .create_index(index(&format!("uk_{collection}_id"), doc! { "id": 1 }, true))
            .await?;
    }
    db.collection::<Document>(TEMPLATES)
        .create_index(index(
            "idx_contract_templates_directory",
            doc! { "enabled": 1, "created_at": -1, "id": -1 },
            false,
        ))
        .await?;
    db.collection::<Document>(APPLICATIONS)
        .create_indexes([
            index("uk_contract_applications_command", doc! { "applicant_id": 1, "command_id": 1 }, true),
            index("uk_contract_applications_number", doc! { "contract_no": 1 }, true),
            index(
                "idx_contract_applications_owner",
                doc! { "applicant_id": 1, "created_at": -1, "id": -1 },
                false,
            ),
        ])
        .await?;
    Ok(())
}

fn index(name: &str, keys: Document, unique: bool) -> IndexModel {
    IndexModel::builder()
        .keys(keys)
        .options(IndexOptions::builder().name(name.to_string()).unique(unique).build())
        .build()
}
