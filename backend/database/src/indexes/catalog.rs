//! 域 D10 `catalog` 的索引声明：product_category、product_brand、unit_of_measure、
//! sku_attribute、sku_attribute_value、product_category_attribute、product(+_revision、
//! _revision_media)、sku(+_revision)、sku_revision_attribute_value、
//! voucher_category_profile_revision。
//!
//! 集合名常量取 `CatalogExt` 关联常量（唯一权威来源，conventions §4.3）：
//! `indexes/` 与 `repository/` 均为冻结声明下的私有子树，模块路径无法互相引用，
//! 关联常量随 trait 公开可达，两侧共用同一值，禁止字面量重复。

use mongodb::{
    bson::{doc, Document},
    options::IndexOptions,
    Database, IndexModel,
};

use crate::repository::extensions::CatalogExt;
use crate::Result;

/// `product_category` 集合名。
pub(crate) const PRODUCT_CATEGORIES: &str = <mongodb::Database as CatalogExt>::PRODUCT_CATEGORIES;
/// `product_brand` 集合名。
pub(crate) const PRODUCT_BRANDS: &str = <mongodb::Database as CatalogExt>::PRODUCT_BRANDS;
/// `unit_of_measure` 集合名。
pub(crate) const UNIT_OF_MEASURES: &str = <mongodb::Database as CatalogExt>::UNIT_OF_MEASURES;
/// `sku_attribute` 集合名。
pub(crate) const SKU_ATTRIBUTES: &str = <mongodb::Database as CatalogExt>::SKU_ATTRIBUTES;
/// `sku_attribute_value` 集合名。
pub(crate) const SKU_ATTRIBUTE_VALUES: &str = <mongodb::Database as CatalogExt>::SKU_ATTRIBUTE_VALUES;
/// `product_category_attribute` 集合名。
pub(crate) const PRODUCT_CATEGORY_ATTRIBUTES: &str =
    <mongodb::Database as CatalogExt>::PRODUCT_CATEGORY_ATTRIBUTES;
/// `product` 集合名。
pub(crate) const PRODUCTS: &str = <mongodb::Database as CatalogExt>::PRODUCTS;
/// `product_revision` 集合名。
pub(crate) const PRODUCT_REVISIONS: &str = <mongodb::Database as CatalogExt>::PRODUCT_REVISIONS;
/// `product_revision_media` 集合名。
pub(crate) const PRODUCT_REVISION_MEDIAS: &str = <mongodb::Database as CatalogExt>::PRODUCT_REVISION_MEDIAS;
/// `sku` 集合名。
pub(crate) const SKUS: &str = <mongodb::Database as CatalogExt>::SKUS;
/// `sku_revision` 集合名。
pub(crate) const SKU_REVISIONS: &str = <mongodb::Database as CatalogExt>::SKU_REVISIONS;
/// `sku_revision_attribute_value` 集合名。
pub(crate) const SKU_REVISION_ATTRIBUTE_VALUES: &str =
    <mongodb::Database as CatalogExt>::SKU_REVISION_ATTRIBUTE_VALUES;
/// `voucher_category_profile_revision` 集合名。
pub(crate) const VOUCHER_CATEGORY_PROFILE_REVISIONS: &str =
    <mongodb::Database as CatalogExt>::VOUCHER_CATEGORY_PROFILE_REVISIONS;

/// 创建本域集合的幂等命名索引。
///
/// 逐条落地数据模型 §6.3「必需约束与索引」：字典与身份类代码使用**全局唯一
/// 索引**（与 accounts 的 code 处理一致）：软删除后仍保留身份，避免复用破坏
/// 恢复语义；`(product_id, specification_signature)` 在全部生命周期记录上永久
/// 唯一，不实现为仅约束启用行的 partial unique index（停用后会产生第二个同签名
/// 稳定 SKU）；条码走非唯一精确查询索引——同一条码允许存在多个在用 SKU，
/// 冲突阻断由 Service 判定（§6.3）。
///
/// # 参数
/// * `db` - 目标 MongoDB 数据库
///
/// # 错误
/// 当已有数据违反唯一约束或 MongoDB 无法创建索引时返回错误。
pub(crate) async fn ensure(db: &Database) -> Result<()> {
    create_indexes(db, PRODUCT_CATEGORIES, product_category_indexes()).await?;
    create_indexes(db, PRODUCT_BRANDS, product_brand_indexes()).await?;
    create_indexes(db, UNIT_OF_MEASURES, unit_of_measure_indexes()).await?;
    create_indexes(db, SKU_ATTRIBUTES, sku_attribute_indexes()).await?;
    create_indexes(db, SKU_ATTRIBUTE_VALUES, sku_attribute_value_indexes()).await?;
    create_indexes(
        db,
        PRODUCT_CATEGORY_ATTRIBUTES,
        product_category_attribute_indexes(),
    )
    .await?;
    create_indexes(db, PRODUCTS, product_indexes()).await?;
    create_indexes(db, PRODUCT_REVISIONS, product_revision_indexes()).await?;
    create_indexes(db, PRODUCT_REVISION_MEDIAS, product_revision_media_indexes()).await?;
    create_indexes(db, SKUS, sku_indexes()).await?;
    create_indexes(db, SKU_REVISIONS, sku_revision_indexes()).await?;
    create_indexes(
        db,
        SKU_REVISION_ATTRIBUTE_VALUES,
        sku_revision_attribute_value_indexes(),
    )
    .await?;
    create_indexes(
        db,
        VOUCHER_CATEGORY_PROFILE_REVISIONS,
        voucher_category_profile_revision_indexes(),
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

/// 返回 `product_category` 的身份约束与树形/启停查询索引。
fn product_category_indexes() -> Vec<IndexModel> {
    vec![
        unique_index("uk_product_categories_category_code", doc! { "category_code": 1 }),
        named_index(
            "idx_product_categories_tree",
            doc! { "parent_category_id": 1, "category_code": 1 },
        ),
        named_index(
            "idx_product_categories_status_tree",
            doc! { "status": 1, "parent_category_id": 1 },
        ),
    ]
}

/// 返回 `product_brand` 的身份约束与启停查询索引。
fn product_brand_indexes() -> Vec<IndexModel> {
    vec![
        unique_index("uk_product_brands_brand_code", doc! { "brand_code": 1 }),
        named_index("idx_product_brands_status", doc! { "status": 1 }),
    ]
}

/// 返回 `unit_of_measure` 的身份约束与启停查询索引。
fn unit_of_measure_indexes() -> Vec<IndexModel> {
    vec![
        unique_index("uk_unit_of_measures_unit_code", doc! { "unit_code": 1 }),
        named_index("idx_unit_of_measures_status", doc! { "status": 1 }),
    ]
}

/// 返回 `sku_attribute` 的身份约束与组合查询索引。
fn sku_attribute_indexes() -> Vec<IndexModel> {
    vec![
        unique_index("uk_sku_attributes_attribute_code", doc! { "attribute_code": 1 }),
        named_index(
            "idx_sku_attributes_status_type",
            doc! { "status": 1, "value_type": 1 },
        ),
    ]
}

/// 返回 `sku_attribute_value` 的身份约束与按属性查询索引。
fn sku_attribute_value_indexes() -> Vec<IndexModel> {
    vec![
        unique_index(
            "uk_sku_attribute_values_attribute_value",
            doc! { "attribute_id": 1, "value_code": 1 },
        ),
        named_index(
            "idx_sku_attribute_values_attribute_sort",
            doc! { "attribute_id": 1, "sort_order": 1 },
        ),
    ]
}

/// 返回 `product_category_attribute` 的组合唯一约束与分类查询索引。
fn product_category_attribute_indexes() -> Vec<IndexModel> {
    vec![
        unique_index(
            "uk_product_category_attributes_relation",
            doc! {
                "category_id": 1,
                "attribute_id": 1,
                "required_flag": 1,
                "sort_order": 1,
            },
        ),
        named_index(
            "idx_product_category_attributes_category",
            doc! { "category_id": 1, "sort_order": 1 },
        ),
    ]
}

/// 返回 `product` 的身份约束与组合查询索引。
fn product_indexes() -> Vec<IndexModel> {
    vec![
        unique_index("uk_products_product_no", doc! { "product_no": 1 }),
        named_index(
            "idx_products_status_kind",
            doc! { "status": 1, "product_kind": 1 },
        ),
    ]
}

/// 返回 `product_revision` 的聚合修订唯一约束。
fn product_revision_indexes() -> Vec<IndexModel> {
    vec![unique_index(
        "uk_product_revisions_revision",
        doc! { "product_id": 1, "revision_no": 1 },
    )]
}

/// 返回 `product_revision_media` 的组合唯一约束与按修订查询索引。
fn product_revision_media_indexes() -> Vec<IndexModel> {
    vec![unique_index(
        "uk_product_revision_medias_media",
        doc! {
            "product_revision_id": 1,
            "media_role": 1,
            "sort_order": 1,
        },
    )]
}

/// 返回 `sku` 的身份约束与全生命周期签名唯一约束。
fn sku_indexes() -> Vec<IndexModel> {
    vec![
        unique_index("uk_skus_sku_no", doc! { "sku_no": 1 }),
        named_index(
            "idx_skus_listing_status",
            doc! { "listing_status": 1, "status": 1, "product_id": 1 },
        ),
        // (product_id, specification_signature) 在全部生命周期记录上永久唯一：
        // 不得实现为仅约束启用行的 partial unique index（数据模型 §6.3）。
        unique_index(
            "uk_skus_product_spec",
            doc! { "product_id": 1, "specification_signature": 1 },
        ),
    ]
}

/// 返回 `sku_revision` 的聚合修订唯一约束、条码精确查询与搜索索引。
fn sku_revision_indexes() -> Vec<IndexModel> {
    vec![
        unique_index(
            "uk_sku_revisions_revision",
            doc! { "sku_id": 1, "revision_no": 1 },
        ),
        // 非空条码规范化精确查询索引（§6.3）：非唯一——同一条码允许存在多个
        // 在用 SKU 修订，冲突阻断转人工由 Service 判定。
        named_index("idx_sku_revisions_barcode", doc! { "barcode": 1 }),
        named_index(
            "idx_sku_revisions_search",
            doc! { "name": 1, "specification": 1, "status": 1 },
        ),
    ]
}

/// 返回 `sku_revision_attribute_value` 的关系唯一约束与正反向查询索引。
fn sku_revision_attribute_value_indexes() -> Vec<IndexModel> {
    vec![
        unique_index(
            "uk_sku_revision_attribute_values_relation",
            doc! { "sku_revision_id": 1, "sku_attribute_id": 1 },
        ),
        named_index(
            "idx_sku_revision_attribute_values_revision",
            doc! { "sku_revision_id": 1, "identity_position": 1 },
        ),
        // §6.3 反向查询索引：按属性值反向定位所属 SKU 修订。
        named_index(
            "idx_sku_revision_attribute_values_reverse",
            doc! { "sku_attribute_value_id": 1, "sku_revision_id": 1 },
        ),
    ]
}

/// 返回 `voucher_category_profile_revision` 的聚合修订唯一约束。
fn voucher_category_profile_revision_indexes() -> Vec<IndexModel> {
    vec![unique_index(
        "uk_voucher_category_profile_revisions_revision",
        doc! { "sku_id": 1, "revision_no": 1 },
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
        product_category_indexes, product_indexes, product_revision_media_indexes, sku_indexes,
        sku_revision_attribute_value_indexes, sku_revision_indexes,
    };

    fn names(indexes: &[mongodb::IndexModel]) -> Vec<String> {
        indexes
            .iter()
            .filter_map(|index| index.options.as_ref()?.name.clone())
            .collect()
    }

    #[test]
    fn product_category_indexes_cover_code_tree_and_status() {
        let indexes = product_category_indexes();

        let code = indexes
            .iter()
            .find(|index| {
                index.options.as_ref().and_then(|options| options.name.as_deref())
                    == Some("uk_product_categories_category_code")
            })
            .unwrap();
        assert_eq!(code.keys, doc! { "category_code": 1 });
        assert_eq!(code.options.as_ref().unwrap().unique, Some(true));

        assert!(names(&indexes).contains(&"idx_product_categories_tree".to_string()));
        assert!(names(&indexes).contains(&"idx_product_categories_status_tree".to_string()));
    }

    #[test]
    fn product_and_sku_identity_indexes_are_globally_unique() {
        let product_indexes = product_indexes();
        let sku_indexes = sku_indexes();

        assert!(product_indexes.iter().any(|index| {
            index.options.as_ref().and_then(|options| options.name.as_deref())
                == Some("uk_products_product_no")
                && index.options.as_ref().and_then(|options| options.unique) == Some(true)
        }));
        let spec = sku_indexes
            .iter()
            .find(|index| {
                index.options.as_ref().and_then(|options| options.name.as_deref())
                    == Some("uk_skus_product_spec")
            })
            .unwrap();
        assert_eq!(spec.keys, doc! { "product_id": 1, "specification_signature": 1 });
        assert_eq!(spec.options.as_ref().unwrap().unique, Some(true));
        assert!(sku_indexes.iter().any(|index| {
            index.options.as_ref().and_then(|options| options.name.as_deref())
                == Some("idx_skus_listing_status")
                && index.keys == doc! { "listing_status": 1, "status": 1, "product_id": 1 }
        }));
    }

    #[test]
    fn sku_revision_barcode_index_is_not_unique_and_search_index_covers_spec() {
        let indexes = sku_revision_indexes();

        let barcode = indexes
            .iter()
            .find(|index| {
                index.options.as_ref().and_then(|options| options.name.as_deref())
                    == Some("idx_sku_revisions_barcode")
            })
            .unwrap();
        assert_eq!(barcode.keys, doc! { "barcode": 1 });
        assert_ne!(barcode.options.as_ref().unwrap().unique, Some(true));

        assert!(indexes
            .iter()
            .any(|index| { index.keys == doc! { "name": 1, "specification": 1, "status": 1 } }));
    }

    #[test]
    fn revision_relation_indexes_cover_media_and_attribute_rows() {
        let media = product_revision_media_indexes();
        assert!(media.iter().any(|index| {
            index.options.as_ref().and_then(|options| options.name.as_deref())
                == Some("uk_product_revision_medias_media")
                && index.options.as_ref().and_then(|options| options.unique) == Some(true)
        }));

        let attribute_rows = sku_revision_attribute_value_indexes();
        assert!(attribute_rows.iter().any(|index| {
            index.options.as_ref().and_then(|options| options.name.as_deref())
                == Some("uk_sku_revision_attribute_values_relation")
        }));
        assert!(attribute_rows
            .iter()
            .any(|index| { index.keys == doc! { "sku_attribute_value_id": 1, "sku_revision_id": 1 } }));
    }
}
