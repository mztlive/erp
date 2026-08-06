//! 域 D24 `supplier_catalog` 的索引声明：supplier_catalog_product(+_revision、
//! _revision_media)、supplier_catalog_sku(+_revision)、supplier_product_mapping、
//! supplier_catalog_intake_batch(+_item)、supplier_offering(+_revision)。
//!
//! 集合名常量取 `SupplierCatalogExt` 关联常量（唯一权威来源，conventions §4.3）：
//! `indexes/` 与 `repository/` 均为冻结声明下的私有子树，模块路径无法互相引用，
//! 关联常量随 trait 公开可达，两侧共用同一值，禁止字面量重复。
//!
//! 逐条落地数据模型 §6.14「必需约束与索引」：
//! - `(supplier_id, supplier_spu_code)` 唯一、`(supplier_catalog_product_id, revision_no)` 唯一、
//!   同一修订 `(media_usage, sort_order)` 唯一；
//! - `(supplier_id, supplier_sku_code)` 唯一（稳定 SKU 未建模 `supplier_id`，
//!   落地为 `(supplier_catalog_product_id, supplier_sku_code)` 唯一，见下方说明）；
//! - `(supplier_catalog_sku_id, revision_no)` 唯一；
//! - `status + source_updated_at`、`availability_status + source_updated_at` 新鲜度索引
//!   （SKU 修订两字段齐备；SPU 级 `status` 位于稳定表、`source_updated_at` 位于
//!   修订表，P1 未做反规范化，无法单集合落地，见 P2 报告）；
//! - 生效映射同一供应商 SKU 唯一（部分唯一索引，理由与回滚方式见注释）；
//! - `(supplier_catalog_sku_id, status)`、`sku_id + status` 映射查询索引；
//! - `(source_type, supplier_id, source_reference)` 批次唯一键；明细按
//!   「批次 + 供应商 SKU + 来源版本」唯一；
//! - 稳定供给身份 `(sku_id, supplier_catalog_sku_id)` 唯一、
//!   `(supplier_offering_id, revision_no)` 唯一、
//!   `sku_id + status + valid_from + valid_to` 按消费/发布时点查询索引。
//!
//! SPU/SKU/供给是身份类稳定表，唯一约束采用**全局唯一索引**（与 accounts 的
//! code 处理一致）：软删除后仍保留身份编码，避免复用破坏来源追溯与恢复语义。

use mongodb::{
    bson::{doc, Document},
    options::IndexOptions,
    Database, IndexModel,
};

use crate::repository::extensions::SupplierCatalogExt;
use crate::Result;

/// `supplier_catalog_product` 集合名。
pub(crate) const SUPPLIER_CATALOG_PRODUCTS: &str =
    <mongodb::Database as SupplierCatalogExt>::SUPPLIER_CATALOG_PRODUCTS;
/// `supplier_catalog_product_revision` 集合名。
pub(crate) const SUPPLIER_CATALOG_PRODUCT_REVISIONS: &str =
    <mongodb::Database as SupplierCatalogExt>::SUPPLIER_CATALOG_PRODUCT_REVISIONS;
/// `supplier_catalog_product_revision_media` 集合名。
pub(crate) const SUPPLIER_CATALOG_PRODUCT_REVISION_MEDIA: &str =
    <mongodb::Database as SupplierCatalogExt>::SUPPLIER_CATALOG_PRODUCT_REVISION_MEDIA;
/// `supplier_catalog_sku` 集合名。
pub(crate) const SUPPLIER_CATALOG_SKUS: &str =
    <mongodb::Database as SupplierCatalogExt>::SUPPLIER_CATALOG_SKUS;
/// `supplier_catalog_sku_revision` 集合名。
pub(crate) const SUPPLIER_CATALOG_SKU_REVISIONS: &str =
    <mongodb::Database as SupplierCatalogExt>::SUPPLIER_CATALOG_SKU_REVISIONS;
/// `supplier_product_mapping` 集合名。
pub(crate) const SUPPLIER_PRODUCT_MAPPINGS: &str =
    <mongodb::Database as SupplierCatalogExt>::SUPPLIER_PRODUCT_MAPPINGS;
/// `supplier_catalog_intake_batch` 集合名。
pub(crate) const SUPPLIER_CATALOG_INTAKE_BATCHES: &str =
    <mongodb::Database as SupplierCatalogExt>::SUPPLIER_CATALOG_INTAKE_BATCHES;
/// `supplier_catalog_intake_item` 集合名。
pub(crate) const SUPPLIER_CATALOG_INTAKE_ITEMS: &str =
    <mongodb::Database as SupplierCatalogExt>::SUPPLIER_CATALOG_INTAKE_ITEMS;
/// `supplier_offering` 集合名。
pub(crate) const SUPPLIER_OFFERINGS: &str = <mongodb::Database as SupplierCatalogExt>::SUPPLIER_OFFERINGS;
/// `supplier_offering_revision` 集合名。
pub(crate) const SUPPLIER_OFFERING_REVISIONS: &str =
    <mongodb::Database as SupplierCatalogExt>::SUPPLIER_OFFERING_REVISIONS;

/// 创建本域集合的幂等命名索引。
///
/// 逐条落地数据模型 §6.14「必需约束与索引」；唯一约束一律用唯一索引表达。
///
/// # 参数
/// * `db` - 目标 MongoDB 数据库
///
/// # 错误
/// 当已有数据违反唯一约束或 MongoDB 无法创建索引时返回错误。
pub(crate) async fn ensure(db: &Database) -> Result<()> {
    create_indexes(db, SUPPLIER_CATALOG_PRODUCTS, supplier_catalog_product_indexes()).await?;
    create_indexes(
        db,
        SUPPLIER_CATALOG_PRODUCT_REVISIONS,
        supplier_catalog_product_revision_indexes(),
    )
    .await?;
    create_indexes(
        db,
        SUPPLIER_CATALOG_PRODUCT_REVISION_MEDIA,
        supplier_catalog_product_revision_media_indexes(),
    )
    .await?;
    create_indexes(db, SUPPLIER_CATALOG_SKUS, supplier_catalog_sku_indexes()).await?;
    create_indexes(
        db,
        SUPPLIER_CATALOG_SKU_REVISIONS,
        supplier_catalog_sku_revision_indexes(),
    )
    .await?;
    create_indexes(db, SUPPLIER_PRODUCT_MAPPINGS, supplier_product_mapping_indexes()).await?;
    create_indexes(
        db,
        SUPPLIER_CATALOG_INTAKE_BATCHES,
        supplier_catalog_intake_batch_indexes(),
    )
    .await?;
    create_indexes(
        db,
        SUPPLIER_CATALOG_INTAKE_ITEMS,
        supplier_catalog_intake_item_indexes(),
    )
    .await?;
    create_indexes(db, SUPPLIER_OFFERINGS, supplier_offering_indexes()).await?;
    create_indexes(
        db,
        SUPPLIER_OFFERING_REVISIONS,
        supplier_offering_revision_indexes(),
    )
    .await?;
    Ok(())
}

/// 为单个集合创建一组幂等命名索引。
async fn create_indexes(db: &Database, collection: &str, indexes: Vec<IndexModel>) -> Result<()> {
    db.collection::<Document>(collection)
        .create_indexes(indexes)
        .await?;
    Ok(())
}

/// 返回 `supplier_catalog_product` 的身份约束与列表查询索引。
///
/// `(supplier_id, supplier_spu_code)` 全局唯一（§6.14）；`supplier_id + status`
/// 支撑供应商商品中心按供应商列表（W21）。
fn supplier_catalog_product_indexes() -> Vec<IndexModel> {
    vec![
        unique_index(
            "uk_supplier_catalog_products_supplier_code",
            doc! { "supplier_id": 1, "supplier_spu_code": 1 },
        ),
        named_index(
            "idx_supplier_catalog_products_supplier_status",
            doc! { "supplier_id": 1, "status": 1 },
        ),
    ]
}

/// 返回 `supplier_catalog_product_revision` 的修订号唯一约束（§6.14）。
fn supplier_catalog_product_revision_indexes() -> Vec<IndexModel> {
    vec![unique_index(
        "uk_supplier_catalog_product_revisions_product_no",
        doc! { "supplier_catalog_product_id": 1, "revision_no": 1 },
    )]
}

/// 返回 `supplier_catalog_product_revision_media` 的图文顺序唯一约束（§6.14）。
fn supplier_catalog_product_revision_media_indexes() -> Vec<IndexModel> {
    vec![unique_index(
        "uk_supplier_catalog_product_revision_media_usage_order",
        doc! {
            "supplier_catalog_product_revision_id": 1,
            "media_usage": 1,
            "sort_order": 1,
        },
    )]
}

/// 返回 `supplier_catalog_sku` 的身份约束与列表查询索引。
///
/// §6.14 要求 `(supplier_id, supplier_sku_code)` 唯一；P1 冻结实体
/// `SupplierCatalogSku` 未建模 `supplier_id`（供应商归属经 SPU 间接表达），
/// 单集合唯一索引只能落地 `(supplier_catalog_product_id, supplier_sku_code)`
/// （同 SPU 下 SKU 编码唯一）。供应商级「同一供应商内 SKU 编码唯一」是
/// `(supplier_id, supplier_spu_code)` 唯一 + 本索引的联合弱约束：同供应商内
/// 跨 SPU 复用 SKU 编码需 P3 聚合校验或地基修订补 `supplier_id`（见 P2 报告）。
fn supplier_catalog_sku_indexes() -> Vec<IndexModel> {
    vec![
        unique_index(
            "uk_supplier_catalog_skus_product_code",
            doc! { "supplier_catalog_product_id": 1, "supplier_sku_code": 1 },
        ),
        named_index(
            "idx_supplier_catalog_skus_product_status",
            doc! { "supplier_catalog_product_id": 1, "status": 1 },
        ),
    ]
}

/// 返回 `supplier_catalog_sku_revision` 的修订号唯一约束与新鲜度索引（§6.14）。
///
/// `availability_status + source_updated_at` 支撑来源 SKU 新鲜度扫描
/// （来源陈旧判定，`availability_status = STALE` 时按 `source_updated_at` 排序）。
fn supplier_catalog_sku_revision_indexes() -> Vec<IndexModel> {
    vec![
        unique_index(
            "uk_supplier_catalog_sku_revisions_sku_no",
            doc! { "supplier_catalog_sku_id": 1, "revision_no": 1 },
        ),
        named_index(
            "idx_supplier_catalog_sku_revisions_availability_freshness",
            doc! { "availability_status": 1, "source_updated_at": 1 },
        ),
    ]
}

/// 返回 `supplier_product_mapping` 的生效唯一约束与查询索引（§6.14）。
///
/// **部分唯一索引理由**：映射以状态表达生命周期（待审核/生效/冲突/停用），
/// 「同一供应商 SKU 同一时点只能映射一个公司 SKU」只约束生效（`ACTIVE`）映射；
/// 历史映射可多条并存（停用后允许新的待审核/生效映射），因此唯一键带
/// `partial_filter_expression { status: "ACTIVE" }`。
/// **回滚方式**：删除本索引，改由 P3 在映射变更事务内按
/// `status = ACTIVE` 聚合校验单一生效映射。
fn supplier_product_mapping_indexes() -> Vec<IndexModel> {
    vec![
        IndexModel::builder()
            .keys(doc! { "supplier_catalog_sku_id": 1 })
            .options(
                IndexOptions::builder()
                    .name("uk_supplier_product_mappings_active_sku".to_string())
                    .unique(true)
                    .partial_filter_expression(doc! { "status": "ACTIVE" })
                    .build(),
            )
            .build(),
        named_index(
            "idx_supplier_product_mappings_sku_status",
            doc! { "supplier_catalog_sku_id": 1, "status": 1 },
        ),
        named_index(
            "idx_supplier_product_mappings_target_sku_status",
            doc! { "sku_id": 1, "status": 1 },
        ),
    ]
}

/// 返回 `supplier_catalog_intake_batch` 的来源键唯一约束与处理队列索引（§6.14）。
///
/// `(source_type, supplier_id, source_reference)` 是批次唯一键（同一来源引用
/// 重复同步不产生第二条）；`status + created_at` 支撑入库处理队列（按状态
/// 过滤、按创建时间排序）。
fn supplier_catalog_intake_batch_indexes() -> Vec<IndexModel> {
    vec![
        unique_index(
            "uk_supplier_catalog_intake_batches_source_key",
            doc! { "source_type": 1, "supplier_id": 1, "source_reference": 1 },
        ),
        named_index(
            "idx_supplier_catalog_intake_batches_status_created",
            doc! { "status": 1, "created_at": 1 },
        ),
    ]
}

/// 返回 `supplier_catalog_intake_item` 的明细唯一约束（§6.14）。
///
/// 明细按「批次 + 供应商 SKU + 来源版本」唯一；`source_revision_token` 缺失时
/// MongoDB 以 `null` 参与唯一比较（同批次同 SKU 无版本号只允许一条），
/// 与来源幂等语义一致，不需要部分唯一索引。
fn supplier_catalog_intake_item_indexes() -> Vec<IndexModel> {
    vec![unique_index(
        "uk_supplier_catalog_intake_items_batch_sku_version",
        doc! {
            "supplier_catalog_intake_batch_id": 1,
            "supplier_sku_code": 1,
            "source_revision_token": 1,
        },
    )]
}

/// 返回 `supplier_offering` 的稳定供给身份约束与消费时点索引（§6.14）。
///
/// `(sku_id, supplier_catalog_sku_id)` 全局唯一（软删除后仍保留供给关系身份）；
/// `sku_id + status + valid_from + valid_to` 按消费时点和发布时点查询；
/// `supplier_id + status` 支撑供应商视角的供给列表（W21）。
fn supplier_offering_indexes() -> Vec<IndexModel> {
    vec![
        unique_index(
            "uk_supplier_offerings_sku_supplier_sku",
            doc! { "sku_id": 1, "supplier_catalog_sku_id": 1 },
        ),
        named_index(
            "idx_supplier_offerings_sku_status_validity",
            doc! { "sku_id": 1, "status": 1, "valid_from": 1, "valid_to": 1 },
        ),
        named_index(
            "idx_supplier_offerings_supplier_status",
            doc! { "supplier_id": 1, "status": 1 },
        ),
    ]
}

/// 返回 `supplier_offering_revision` 的修订号唯一约束（§6.14）。
fn supplier_offering_revision_indexes() -> Vec<IndexModel> {
    vec![unique_index(
        "uk_supplier_offering_revisions_offering_no",
        doc! { "supplier_offering_id": 1, "revision_no": 1 },
    )]
}

/// 构建命名普通索引。
fn named_index(name: impl Into<String>, keys: Document) -> IndexModel {
    IndexModel::builder()
        .keys(keys)
        .options(IndexOptions::builder().name(name.into()).build())
        .build()
}

/// 构建命名唯一索引。
fn unique_index(name: impl Into<String>, keys: Document) -> IndexModel {
    IndexModel::builder()
        .keys(keys)
        .options(IndexOptions::builder().name(name.into()).unique(true).build())
        .build()
}

#[cfg(test)]
mod tests {
    use mongodb::bson::doc;

    use super::{
        supplier_catalog_intake_batch_indexes, supplier_catalog_intake_item_indexes,
        supplier_catalog_product_indexes, supplier_catalog_sku_indexes,
        supplier_catalog_sku_revision_indexes, supplier_offering_indexes, supplier_product_mapping_indexes,
    };

    #[test]
    fn product_identity_is_globally_unique_per_supplier() {
        let indexes = supplier_catalog_product_indexes();

        let identity = indexes
            .iter()
            .find(|index| {
                index.options.as_ref().and_then(|options| options.name.as_deref())
                    == Some("uk_supplier_catalog_products_supplier_code")
            })
            .unwrap();
        assert_eq!(identity.keys, doc! { "supplier_id": 1, "supplier_spu_code": 1 });
        assert_eq!(identity.options.as_ref().unwrap().unique, Some(true));

        assert!(indexes
            .iter()
            .any(|index| index.keys == doc! { "supplier_id": 1, "status": 1 }));
    }

    #[test]
    fn sku_identity_unique_per_product_and_freshness_index_exists() {
        let indexes = supplier_catalog_sku_indexes();

        let identity = indexes
            .iter()
            .find(|index| {
                index.options.as_ref().and_then(|options| options.name.as_deref())
                    == Some("uk_supplier_catalog_skus_product_code")
            })
            .unwrap();
        assert_eq!(
            identity.keys,
            doc! { "supplier_catalog_product_id": 1, "supplier_sku_code": 1 }
        );
        assert_eq!(identity.options.as_ref().unwrap().unique, Some(true));

        let revision_indexes = supplier_catalog_sku_revision_indexes();
        assert!(revision_indexes
            .iter()
            .any(|index| { index.keys == doc! { "availability_status": 1, "source_updated_at": 1 } }));
    }

    #[test]
    fn active_mapping_unique_is_partial_and_query_indexes_cover_both_directions() {
        let indexes = supplier_product_mapping_indexes();

        let active = indexes
            .iter()
            .find(|index| {
                index.options.as_ref().and_then(|options| options.name.as_deref())
                    == Some("uk_supplier_product_mappings_active_sku")
            })
            .unwrap();
        assert_eq!(active.keys, doc! { "supplier_catalog_sku_id": 1 });
        assert_eq!(active.options.as_ref().unwrap().unique, Some(true));
        assert_eq!(
            active.options.as_ref().unwrap().partial_filter_expression,
            Some(doc! { "status": "ACTIVE" })
        );

        assert!(indexes
            .iter()
            .any(|index| index.keys == doc! { "supplier_catalog_sku_id": 1, "status": 1 }));
        assert!(indexes
            .iter()
            .any(|index| index.keys == doc! { "sku_id": 1, "status": 1 }));
    }

    #[test]
    fn intake_batch_source_key_unique_and_items_unique_by_sku_version() {
        let batch_indexes = supplier_catalog_intake_batch_indexes();
        assert!(batch_indexes.iter().any(|index| {
            index.options.as_ref().and_then(|options| options.name.as_deref())
                == Some("uk_supplier_catalog_intake_batches_source_key")
                && index.options.as_ref().and_then(|options| options.unique) == Some(true)
        }));
        assert!(batch_indexes
            .iter()
            .any(|index| index.keys == doc! { "status": 1, "created_at": 1 }));

        let item_indexes = supplier_catalog_intake_item_indexes();
        assert!(item_indexes.iter().any(|index| {
            index.keys
                == doc! {
                    "supplier_catalog_intake_batch_id": 1,
                    "supplier_sku_code": 1,
                    "source_revision_token": 1,
                }
                && index.options.as_ref().and_then(|options| options.unique) == Some(true)
        }));
    }

    #[test]
    fn offering_identity_unique_and_validity_index_covers_sku_status_window() {
        let indexes = supplier_offering_indexes();

        assert!(indexes.iter().any(|index| {
            index.options.as_ref().and_then(|options| options.name.as_deref())
                == Some("uk_supplier_offerings_sku_supplier_sku")
                && index.options.as_ref().and_then(|options| options.unique) == Some(true)
        }));
        assert!(indexes.iter().any(|index| {
            index.keys == doc! { "sku_id": 1, "status": 1, "valid_from": 1, "valid_to": 1 }
        }));
        assert!(indexes
            .iter()
            .any(|index| index.keys == doc! { "supplier_id": 1, "status": 1 }));
    }
}
