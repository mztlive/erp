//! 新增集合唯一身份、领号重放和分页索引。

use mongodb::bson::{Document, doc};
use mongodb::options::IndexOptions;
use mongodb::{Database, IndexModel};
use persistence_core::Result;

use crate::repository::templates::{APPLICATIONS, COMPANY_NUMBERING, COUNTERS, TEMPLATES};

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
