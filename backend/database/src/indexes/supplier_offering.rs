//! 域 D24 供应商供给索引。
//!
//! 项目尚未上线，不提供旧供应商商品/SKU/映射集合的数据兼容；部署前直接清空
//! 旧集合。新模型只创建供给、商业条款修订、实时可供投影和幂等命令索引。

use mongodb::{
    bson::{doc, Document},
    options::IndexOptions,
    Database, IndexModel,
};

use crate::repository::extensions::SupplierOfferingExt;
use crate::Result;

const OFFERINGS: &str = <Database as SupplierOfferingExt>::SUPPLIER_OFFERINGS;
const OFFERING_REVISIONS: &str = <Database as SupplierOfferingExt>::SUPPLIER_OFFERING_REVISIONS;
const OFFERING_AVAILABILITIES: &str = <Database as SupplierOfferingExt>::SUPPLIER_OFFERING_AVAILABILITIES;
const OFFERING_COMMANDS: &str = <Database as SupplierOfferingExt>::SUPPLIER_OFFERING_COMMANDS;

/// 创建供应商供给域的幂等命名索引。
///
/// # 参数
/// * `db` - 目标数据库
///
/// # 返回
/// 全部索引创建成功返回 `Ok(())`。
///
/// # 错误
/// 已有数据违反唯一约束或 MongoDB 失败时返回错误。
pub(crate) async fn ensure(db: &Database) -> Result<()> {
    create_indexes(db, OFFERINGS, offering_indexes()).await?;
    create_indexes(db, OFFERING_REVISIONS, revision_indexes()).await?;
    create_indexes(db, OFFERING_AVAILABILITIES, availability_indexes()).await?;
    create_indexes(db, OFFERING_COMMANDS, command_indexes()).await
}

async fn create_indexes(db: &Database, collection: &str, indexes: Vec<IndexModel>) -> Result<()> {
    db.collection::<Document>(collection)
        .create_indexes(indexes)
        .await?;
    Ok(())
}

fn offering_indexes() -> Vec<IndexModel> {
    vec![
        unique_index(
            "uk_supplier_offerings_supplier_sku",
            doc! { "supplier_id": 1, "supplier_sku_code": 1 },
        ),
        named_index(
            "idx_supplier_offerings_sku_status",
            doc! { "sku_id": 1, "status": 1 },
        ),
        named_index(
            "idx_supplier_offerings_supplier_status",
            doc! { "supplier_id": 1, "status": 1 },
        ),
        named_index(
            "idx_supplier_offerings_source_connection",
            doc! { "source_connection_id": 1, "status": 1 },
        ),
    ]
}

fn revision_indexes() -> Vec<IndexModel> {
    vec![
        unique_index(
            "uk_supplier_offering_revisions_offering_no",
            doc! { "supplier_offering_id": 1, "revision_no": 1 },
        ),
        named_index(
            "idx_supplier_offering_revisions_validity",
            doc! { "supplier_offering_id": 1, "valid_from": -1, "valid_to": 1 },
        ),
    ]
}

fn availability_indexes() -> Vec<IndexModel> {
    vec![
        unique_index(
            "uk_supplier_offering_availabilities_offering",
            doc! { "supplier_offering_id": 1 },
        ),
        named_index(
            "idx_supplier_offering_availabilities_freshness",
            doc! { "availability_status": 1, "source_updated_at": 1 },
        ),
    ]
}

fn command_indexes() -> Vec<IndexModel> {
    vec![unique_index(
        "uk_supplier_offering_commands_idempotency_key",
        doc! { "idempotency_key": 1 },
    )]
}

fn named_index(name: impl Into<String>, keys: Document) -> IndexModel {
    IndexModel::builder()
        .keys(keys)
        .options(IndexOptions::builder().name(name.into()).build())
        .build()
}

fn unique_index(name: impl Into<String>, keys: Document) -> IndexModel {
    IndexModel::builder()
        .keys(keys)
        .options(IndexOptions::builder().name(name.into()).unique(true).build())
        .build()
}

#[cfg(test)]
mod tests {
    use mongodb::bson::doc;

    use super::{availability_indexes, offering_indexes, revision_indexes};

    #[test]
    fn supplier_sku_identity_is_unique_without_mapping() {
        let indexes = offering_indexes();
        let identity = indexes
            .iter()
            .find(|index| {
                index.options.as_ref().and_then(|value| value.name.as_deref())
                    == Some("uk_supplier_offerings_supplier_sku")
            })
            .unwrap();
        assert_eq!(identity.keys, doc! { "supplier_id": 1, "supplier_sku_code": 1 });
        assert_eq!(identity.options.as_ref().unwrap().unique, Some(true));
    }

    #[test]
    fn revision_and_availability_have_separate_unique_keys() {
        assert!(revision_indexes()
            .iter()
            .any(|index| { index.keys == doc! { "supplier_offering_id": 1, "revision_no": 1 } }));
        assert!(availability_indexes().iter().any(|index| {
            index.keys == doc! { "supplier_offering_id": 1 }
                && index.options.as_ref().and_then(|value| value.unique) == Some(true)
        }));
    }
}
