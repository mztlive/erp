//! 域 D27 `projection` 的索引声明：sales_order_projection(+_revision、_delivery)。
//!
//! 集合名常量取 `ProjectionExt` 关联常量（唯一权威来源，conventions §4.3）：
//! `indexes/` 与 `repository/` 均为冻结声明下的私有子树，模块路径无法互相引用，
//! 关联常量随 trait 公开可达，两侧共用同一值，禁止字面量重复。

use mongodb::{
    bson::{doc, Document},
    options::IndexOptions,
    Database, IndexModel,
};

use crate::repository::extensions::ProjectionExt;
use crate::Result;

/// `sales_order_projection` 集合名。
pub(crate) const SALES_ORDER_PROJECTIONS: &str =
    <mongodb::Database as ProjectionExt>::SALES_ORDER_PROJECTIONS;
/// `sales_order_projection_revision` 集合名。
pub(crate) const SALES_ORDER_PROJECTION_REVISIONS: &str =
    <mongodb::Database as ProjectionExt>::SALES_ORDER_PROJECTION_REVISIONS;
/// `sales_order_projection_delivery` 集合名。
pub(crate) const SALES_ORDER_PROJECTION_DELIVERIES: &str =
    <mongodb::Database as ProjectionExt>::SALES_ORDER_PROJECTION_DELIVERIES;

/// 创建本域集合的幂等命名索引。
///
/// 逐条落地数据模型 §6.16「必需约束与索引」：
/// - `(sales_order_id, target_mall_id)` 唯一稳定投影；
/// - `(projection_id, revision_no)` 唯一；
/// - 下发状态查询索引。
///
/// **§6.16 幂等键说明（冻结实体偏差）**：文档要求 `(sales_order_revision_id,
/// target_mall_id)` 唯一（幂等键「ERP 销售单号 + ERP 销售单版本 + 目标商城」），
/// 但 P1 冻结的 `sales_order_projection_revision` 实体不携带 `target_mall_id`
/// （商城归属由 `projection_id → sales_order_projection` 推导），该复合唯一索引
/// 无法建在修订集合上。幂等防护改在**下发记录集合**落地：同一投影修订对同一
/// 商城只允许一条投递记录（`uk_sales_order_projection_deliveries_revision_mall`），
/// 配合投影 `(sales_order_id, target_mall_id)` 与修订 `(projection_id, revision_no)`
/// 两个唯一索引，共同保证「同一 ERP 销售版本不会向同一商城重复下发」。
///
/// 投影版本为不可变修订（§4.4），只追加不覆盖，身份与幂等约束全部使用
/// 全局唯一索引，不采用部分唯一索引。
///
/// # 参数
/// * `db` - 目标 MongoDB 数据库
///
/// # 错误
/// 当已有数据违反唯一约束或 MongoDB 无法创建索引时返回错误。
pub(crate) async fn ensure(db: &Database) -> Result<()> {
    create_indexes(db, SALES_ORDER_PROJECTIONS, sales_order_projection_indexes()).await?;
    create_indexes(
        db,
        SALES_ORDER_PROJECTION_REVISIONS,
        sales_order_projection_revision_indexes(),
    )
    .await?;
    create_indexes(
        db,
        SALES_ORDER_PROJECTION_DELIVERIES,
        sales_order_projection_delivery_indexes(),
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

/// 返回 `sales_order_projection` 的稳定投影唯一约束索引。
fn sales_order_projection_indexes() -> Vec<IndexModel> {
    vec![unique_index(
        "uk_sales_order_projections_order_mall",
        doc! { "sales_order_id": 1, "target_mall_id": 1 },
    )]
}

/// 返回 `sales_order_projection_revision` 的修订唯一约束索引。
fn sales_order_projection_revision_indexes() -> Vec<IndexModel> {
    vec![unique_index(
        "uk_sales_order_projection_revisions_projection_revision",
        doc! { "projection_id": 1, "revision_no": 1 },
    )]
}

/// 返回 `sales_order_projection_delivery` 的幂等与状态查询索引。
fn sales_order_projection_delivery_indexes() -> Vec<IndexModel> {
    vec![
        unique_index(
            "uk_sales_order_projection_deliveries_revision_mall",
            doc! { "projection_revision_id": 1, "target_mall_id": 1 },
        ),
        unique_index(
            "uk_sales_order_projection_deliveries_message_key",
            doc! { "message_key": 1 },
        ),
        named_index(
            "idx_sales_order_projection_deliveries_processable",
            doc! { "status": 1, "next_attempt_at": 1, "created_at": 1 },
        ),
    ]
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
        sales_order_projection_delivery_indexes, sales_order_projection_indexes,
        sales_order_projection_revision_indexes,
    };

    #[test]
    fn projection_order_mall_index_is_unique() {
        let indexes = sales_order_projection_indexes();

        let order_mall = indexes
            .iter()
            .find(|index| {
                index.options.as_ref().and_then(|options| options.name.as_deref())
                    == Some("uk_sales_order_projections_order_mall")
            })
            .unwrap();
        assert_eq!(order_mall.keys, doc! { "sales_order_id": 1, "target_mall_id": 1 });
        assert_eq!(order_mall.options.as_ref().unwrap().unique, Some(true));
    }

    #[test]
    fn revision_index_covers_revision_no_uniqueness() {
        let indexes = sales_order_projection_revision_indexes();

        assert!(indexes.iter().any(|index| {
            index.options.as_ref().and_then(|options| options.name.as_deref())
                == Some("uk_sales_order_projection_revisions_projection_revision")
                && index.keys == doc! { "projection_id": 1, "revision_no": 1 }
                && index.options.as_ref().and_then(|options| options.unique) == Some(true)
        }));
    }

    #[test]
    fn delivery_indexes_cover_idempotency_and_status_query() {
        let indexes = sales_order_projection_delivery_indexes();

        assert!(indexes.iter().any(|index| {
            index.options.as_ref().and_then(|options| options.name.as_deref())
                == Some("uk_sales_order_projection_deliveries_revision_mall")
                && index.keys == doc! { "projection_revision_id": 1, "target_mall_id": 1 }
                && index.options.as_ref().and_then(|options| options.unique) == Some(true)
        }));
        assert!(indexes.iter().any(|index| {
            index.keys == doc! { "status": 1, "next_attempt_at": 1, "created_at": 1 }
                && index.options.as_ref().and_then(|options| options.name.as_deref())
                    == Some("idx_sales_order_projection_deliveries_processable")
        }));
        assert!(indexes.iter().any(|index| {
            index.keys == doc! { "message_key": 1 }
                && index.options.as_ref().and_then(|options| options.name.as_deref())
                    == Some("uk_sales_order_projection_deliveries_message_key")
                && index.options.as_ref().and_then(|options| options.unique) == Some(true)
        }));
    }
}
