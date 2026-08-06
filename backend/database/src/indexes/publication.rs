//! 域 D26 `publication` 的索引声明：product_publication(+_revision、_revision_media)、
//! product_publication_delivery。
//!
//! 集合名常量取 `PublicationExt` 关联常量（唯一权威来源，conventions §4.3）：
//! `indexes/` 与 `repository/` 均为冻结声明下的私有子树，模块路径无法互相引用，
//! 关联常量随 trait 公开可达，两侧共用同一值，禁止字面量重复。

use mongodb::{
    bson::{doc, Document},
    options::IndexOptions,
    Database, IndexModel,
};

use crate::repository::extensions::PublicationExt;
use crate::Result;

/// `product_publication` 集合名。
pub(crate) const PRODUCT_PUBLICATIONS: &str = <mongodb::Database as PublicationExt>::PRODUCT_PUBLICATIONS;
/// `product_publication_revision` 集合名。
pub(crate) const PRODUCT_PUBLICATION_REVISIONS: &str =
    <mongodb::Database as PublicationExt>::PRODUCT_PUBLICATION_REVISIONS;
/// `product_publication_revision_media` 集合名。
pub(crate) const PRODUCT_PUBLICATION_REVISION_MEDIA: &str =
    <mongodb::Database as PublicationExt>::PRODUCT_PUBLICATION_REVISION_MEDIA;
/// `product_publication_delivery` 集合名。
pub(crate) const PRODUCT_PUBLICATION_DELIVERIES: &str =
    <mongodb::Database as PublicationExt>::PRODUCT_PUBLICATION_DELIVERIES;

/// 创建本域集合的幂等命名索引。
///
/// 逐条落地数据模型 §6.15「必需约束与索引」：
/// - `(sku_id, target_mall_id)` 唯一稳定发布；
/// - `(product_publication_id, revision_no)` 唯一；该唯一约束与发布主表
///   `(sku_id, target_mall_id)` 唯一共同推导出对外幂等键
///   `(sku_id, revision_no, target_mall_id)`（§6.15）；
/// - `(product_publication_revision_id, media_role, sort_no)` 唯一；
/// - 投递状态查询索引。
///
/// # 参数
/// * `db` - 目标 MongoDB 数据库
///
/// # 错误
/// 当已有数据违反唯一约束或 MongoDB 无法创建索引时返回错误。
pub(crate) async fn ensure(db: &Database) -> Result<()> {
    create_indexes(db, PRODUCT_PUBLICATIONS, product_publication_indexes()).await?;
    create_indexes(
        db,
        PRODUCT_PUBLICATION_REVISIONS,
        product_publication_revision_indexes(),
    )
    .await?;
    create_indexes(
        db,
        PRODUCT_PUBLICATION_REVISION_MEDIA,
        product_publication_revision_media_indexes(),
    )
    .await?;
    create_indexes(
        db,
        PRODUCT_PUBLICATION_DELIVERIES,
        product_publication_delivery_indexes(),
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

/// 返回 `product_publication` 的稳定发布唯一约束与列表查询索引。
fn product_publication_indexes() -> Vec<IndexModel> {
    vec![
        unique_index(
            "uk_product_publications_sku_mall",
            doc! { "sku_id": 1, "target_mall_id": 1 },
        ),
        named_index("idx_product_publications_status", doc! { "status": 1 }),
    ]
}

/// 返回 `product_publication_revision` 的修订唯一约束索引。
fn product_publication_revision_indexes() -> Vec<IndexModel> {
    vec![unique_index(
        "uk_product_publication_revisions_publication_revision",
        doc! { "product_publication_id": 1, "revision_no": 1 },
    )]
}

/// 返回 `product_publication_revision_media` 的媒体行唯一约束索引。
fn product_publication_revision_media_indexes() -> Vec<IndexModel> {
    vec![unique_index(
        "uk_product_publication_revision_media_revision_role_sort",
        doc! {
            "product_publication_revision_id": 1,
            "media_role": 1,
            "sort_no": 1,
        },
    )]
}

/// 返回 `product_publication_delivery` 的投递状态查询索引。
fn product_publication_delivery_indexes() -> Vec<IndexModel> {
    vec![named_index(
        "idx_product_publication_deliveries_status",
        doc! { "delivery_status": 1 },
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
        product_publication_delivery_indexes, product_publication_indexes,
        product_publication_revision_indexes, product_publication_revision_media_indexes,
    };

    #[test]
    fn publication_sku_mall_index_is_unique_and_status_index_covers_list() {
        let indexes = product_publication_indexes();

        let sku_mall = indexes
            .iter()
            .find(|index| {
                index.options.as_ref().and_then(|options| options.name.as_deref())
                    == Some("uk_product_publications_sku_mall")
            })
            .unwrap();
        assert_eq!(sku_mall.keys, doc! { "sku_id": 1, "target_mall_id": 1 });
        assert_eq!(sku_mall.options.as_ref().unwrap().unique, Some(true));

        assert!(indexes.iter().any(|index| index.keys == doc! { "status": 1 }));
    }

    #[test]
    fn revision_index_is_unique_per_publication() {
        let indexes = product_publication_revision_indexes();

        let revision = indexes
            .iter()
            .find(|index| {
                index.options.as_ref().and_then(|options| options.name.as_deref())
                    == Some("uk_product_publication_revisions_publication_revision")
            })
            .unwrap();
        assert_eq!(
            revision.keys,
            doc! { "product_publication_id": 1, "revision_no": 1 }
        );
        assert_eq!(revision.options.as_ref().unwrap().unique, Some(true));
    }

    #[test]
    fn media_index_covers_revision_role_and_sort() {
        let indexes = product_publication_revision_media_indexes();

        let media = indexes
            .iter()
            .find(|index| {
                index.options.as_ref().and_then(|options| options.name.as_deref())
                    == Some("uk_product_publication_revision_media_revision_role_sort")
            })
            .unwrap();
        assert_eq!(
            media.keys,
            doc! {
                "product_publication_revision_id": 1,
                "media_role": 1,
                "sort_no": 1,
            }
        );
        assert_eq!(media.options.as_ref().unwrap().unique, Some(true));
    }

    #[test]
    fn delivery_status_index_covers_status_query() {
        let indexes = product_publication_delivery_indexes();

        assert!(indexes.iter().any(|index| {
            index.keys == doc! { "delivery_status": 1 }
                && index.options.as_ref().and_then(|options| options.name.as_deref())
                    == Some("idx_product_publication_deliveries_status")
        }));
    }
}
