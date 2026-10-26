//! 域 D24 供应商供给索引。
//!
//! 项目尚未上线，不提供旧供应商商品/SKU/映射集合的数据兼容；部署前直接清空
//! 旧集合。新模型只创建供给、商业条款修订、实时可供投影和幂等命令索引。

use mongodb::bson::{Document, doc};
use mongodb::options::IndexOptions;
use mongodb::{Database, IndexModel};
use persistence_core::Result;
use persistence_core::indexes::{named_index, unique_index};

use crate::repository::SupplierOfferingExt;

const OFFERINGS: &str = <Database as SupplierOfferingExt>::SUPPLIER_OFFERINGS;
const OFFERING_REVISIONS: &str = <Database as SupplierOfferingExt>::SUPPLIER_OFFERING_REVISIONS;
const OFFERING_AVAILABILITIES: &str = <Database as SupplierOfferingExt>::SUPPLIER_OFFERING_AVAILABILITIES;
const OFFERING_COMMANDS: &str = <Database as SupplierOfferingExt>::SUPPLIER_OFFERING_COMMANDS;
const DELIVERY_TEMPLATES: &str = <Database as SupplierOfferingExt>::SUPPLIER_DELIVERY_TEMPLATES;
const SKU_DELIVERY_COVERAGES: &str = <Database as SupplierOfferingExt>::SKU_DELIVERY_COVERAGES;

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
    create_indexes(db, OFFERING_COMMANDS, command_indexes()).await?;
    create_indexes(db, DELIVERY_TEMPLATES, delivery_template_indexes()).await?;
    create_indexes(db, SKU_DELIVERY_COVERAGES, sku_delivery_coverage_indexes()).await
}

async fn create_indexes(db: &Database, collection: &str, indexes: Vec<IndexModel>) -> Result<()> {
    db.collection::<Document>(collection).create_indexes(indexes).await?;
    Ok(())
}

fn offering_indexes() -> Vec<IndexModel> {
    vec![
        unique_index("uk_supplier_offerings_id", doc! { "id": 1 }),
        unique_index("uk_supplier_offerings_supplier_sku", doc! { "supplier_id": 1, "supplier_sku_code": 1 }),
        named_index("idx_supplier_offerings_sku_status", doc! { "sku_id": 1, "status": 1 }),
        named_index("idx_supplier_offerings_supplier_status", doc! { "supplier_id": 1, "status": 1 }),
        named_index(
            "idx_supplier_offerings_source_connection",
            doc! { "source_connection_id": 1, "status": 1 },
        ),
        named_index("idx_supplier_offerings_source_status", doc! { "source_type": 1, "status": 1 }),
        named_index(
            "idx_supplier_offerings_maintainer_org",
            doc! { "maintainer_user_id": 1, "business_org_unit_id": 1, "created_at": -1, "id": 1 },
        ),
        named_index("idx_supplier_offerings_delivery_template", doc! { "delivery_template_id": 1 }),
        named_index("idx_supplier_offerings_updated_at", doc! { "updated_at": 1 }),
    ]
}

fn revision_indexes() -> Vec<IndexModel> {
    vec![
        unique_index("uk_supplier_offering_revisions_id", doc! { "id": 1 }),
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
        named_index(
            "idx_supplier_availability_expiry",
            doc! {"availability_status":1,"valid_until":1,"id":1},
        ),
        unique_index("uk_supplier_offering_availabilities_id", doc! { "id": 1 }),
        unique_index("uk_supplier_offering_availabilities_offering", doc! { "supplier_offering_id": 1 }),
        named_index(
            "idx_supplier_offering_availabilities_freshness",
            doc! { "availability_status": 1, "source_updated_at": 1 },
        ),
        named_index("idx_supplier_offering_availabilities_updated_at", doc! { "updated_at": 1 }),
    ]
}

fn command_indexes() -> Vec<IndexModel> {
    vec![
        unique_index("uk_supplier_offering_commands_id", doc! { "id": 1 }),
        unique_index("uk_supplier_offering_commands_idempotency_key", doc! { "idempotency_key": 1 }),
    ]
}

fn delivery_template_indexes() -> Vec<IndexModel> {
    let default_only = IndexOptions::builder()
        .name("uk_supplier_delivery_templates_default".to_string())
        .unique(true)
        .partial_filter_expression(doc! { "is_default": true, "deleted_at": 0_i64 })
        .build();
    vec![
        unique_index("uk_supplier_delivery_templates_id", doc! { "id": 1 }),
        unique_index(
            "uk_supplier_delivery_templates_supplier_name",
            doc! { "supplier_id": 1, "name": 1, "deleted_at": 1 },
        ),
        IndexModel::builder().keys(doc! { "supplier_id": 1 }).options(default_only).build(),
        named_index("idx_supplier_delivery_templates_updated_at", doc! { "updated_at": 1 }),
    ]
}

/// SKU 配送覆盖：四个地区集合供商品池筛选，供应商供模板变化标记，待重算与到期日供后台刷新。
fn sku_delivery_coverage_indexes() -> Vec<IndexModel> {
    let mut indexes = ["included.full", "included.partial", "reachable.full", "reachable.partial"]
        .into_iter()
        .map(|field| {
            named_index(format!("idx_sku_delivery_coverages_{}", field.replace('.', "_")), doc! { field: 1 })
        })
        .collect::<Vec<_>>();
    indexes.push(named_index("idx_sku_delivery_coverages_suppliers", doc! { "supplier_ids": 1 }));
    indexes.push(partial_index(
        "idx_sku_delivery_coverages_dirty",
        doc! { "dirty": 1 },
        doc! { "dirty": true },
    ));
    indexes.push(partial_index(
        "idx_sku_delivery_coverages_next_change",
        doc! { "next_change_date": 1 },
        doc! { "next_change_date": { "$type": "string" } },
    ));
    indexes
}

fn partial_index(name: &str, keys: Document, filter: Document) -> IndexModel {
    let options = IndexOptions::builder().name(name.to_string()).partial_filter_expression(filter).build();
    IndexModel::builder().keys(keys).options(options).build()
}

#[cfg(test)]
mod tests {
    /// 按稳定主键查询与乐观锁更新依赖 `id` 唯一索引，否则退化为集合扫描。
    #[test]
    fn collections_look_up_by_unique_stable_id() {
        let indexes = super::command_indexes();
        assert!(indexes.iter().any(|index| {
            index.keys == mongodb::bson::doc! { "id": 1 }
                && index.options.as_ref().and_then(|options| options.unique) == Some(true)
        }));
    }

    use mongodb::bson::doc;

    use super::{availability_indexes, delivery_template_indexes, offering_indexes, revision_indexes};

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
        assert!(
            revision_indexes()
                .iter()
                .any(|index| { index.keys == doc! { "supplier_offering_id": 1, "revision_no": 1 } })
        );
        assert!(availability_indexes().iter().any(|index| {
            index.keys == doc! { "supplier_offering_id": 1 }
                && index.options.as_ref().and_then(|value| value.unique) == Some(true)
        }));
    }

    #[test]
    fn offering_source_filter_has_an_index() {
        assert!(offering_indexes().iter().any(|index| index.keys == doc! { "source_type": 1, "status": 1 }));
        assert!(offering_indexes().iter().any(|index| {
            index.keys
                == doc! { "maintainer_user_id": 1, "business_org_unit_id": 1, "created_at": -1, "id": 1 }
        }));
    }

    #[test]
    fn offering_collections_look_up_by_stable_id() {
        for indexes in
            [offering_indexes(), revision_indexes(), availability_indexes(), delivery_template_indexes()]
        {
            assert!(indexes.iter().any(|index| {
                index.keys == doc! { "id": 1 }
                    && index.options.as_ref().and_then(|value| value.unique) == Some(true)
            }));
        }
    }

    #[test]
    fn coverage_filters_and_change_scans_have_indexes() {
        let coverage = super::sku_delivery_coverage_indexes();
        for field in
            ["included.full", "included.partial", "reachable.full", "reachable.partial", "supplier_ids"]
        {
            assert!(coverage.iter().any(|index| index.keys == doc! { field: 1 }), "{field}");
        }
        let dirty = coverage.iter().find(|index| index.keys == doc! { "dirty": 1 }).unwrap();
        assert_eq!(dirty.options.as_ref().unwrap().partial_filter_expression, Some(doc! { "dirty": true }));
        assert!(offering_indexes().iter().any(|index| index.keys == doc! { "updated_at": 1 }));
        assert!(availability_indexes().iter().any(|index| index.keys == doc! { "updated_at": 1 }));
        assert!(delivery_template_indexes().iter().any(|index| index.keys == doc! { "updated_at": 1 }));
    }

    #[test]
    fn delivery_templates_keep_names_and_default_unique_per_supplier() {
        let indexes = delivery_template_indexes();
        assert!(indexes.iter().any(|index| {
            index.keys == doc! { "supplier_id": 1, "name": 1, "deleted_at": 1 }
                && index.options.as_ref().and_then(|value| value.unique) == Some(true)
        }));
        let default_index = indexes.iter().find(|index| index.keys == doc! { "supplier_id": 1 }).unwrap();
        let options = default_index.options.as_ref().unwrap();
        assert_eq!(options.unique, Some(true));
        assert_eq!(options.partial_filter_expression, Some(doc! { "is_default": true, "deleted_at": 0_i64 }));
        assert!(offering_indexes().iter().any(|index| index.keys == doc! { "delivery_template_id": 1 }));
    }
}
