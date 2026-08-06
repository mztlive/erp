//! 域 D14 `sales_review` 的索引声明：sales_order_review、procurement_confirmation
//! (+_line)、sales_change_order、sales_change_submission(+_line)、sales_change_review
//! （页面：W05、W07）。
//!
//! 集合名常量取 `SalesReviewExt` 关联常量（唯一权威来源，conventions §4.3）：
//! `indexes/` 与 `repository/` 均为冻结声明下的私有子树，模块路径无法互相引用，
//! 关联常量随 trait 公开可达，两侧共用同一值，禁止字面量重复。

use mongodb::{
    bson::{doc, Document},
    options::IndexOptions,
    Database, IndexModel,
};

use crate::repository::extensions::SalesReviewExt;
use crate::Result;

/// `sales_order_review` 集合名。
pub(crate) const SALES_ORDER_REVIEWS: &str = <mongodb::Database as SalesReviewExt>::SALES_ORDER_REVIEWS;
/// `procurement_confirmation` 集合名。
pub(crate) const PROCUREMENT_CONFIRMATIONS: &str =
    <mongodb::Database as SalesReviewExt>::PROCUREMENT_CONFIRMATIONS;
/// `procurement_confirmation_line` 集合名。
pub(crate) const PROCUREMENT_CONFIRMATION_LINES: &str =
    <mongodb::Database as SalesReviewExt>::PROCUREMENT_CONFIRMATION_LINES;
/// `sales_change_order` 集合名。
pub(crate) const SALES_CHANGE_ORDERS: &str = <mongodb::Database as SalesReviewExt>::SALES_CHANGE_ORDERS;
/// `sales_change_submission` 集合名。
pub(crate) const SALES_CHANGE_SUBMISSIONS: &str =
    <mongodb::Database as SalesReviewExt>::SALES_CHANGE_SUBMISSIONS;
/// `sales_change_submission_line` 集合名。
pub(crate) const SALES_CHANGE_SUBMISSION_LINES: &str =
    <mongodb::Database as SalesReviewExt>::SALES_CHANGE_SUBMISSION_LINES;
/// `sales_change_review` 集合名。
pub(crate) const SALES_CHANGE_REVIEWS: &str = <mongodb::Database as SalesReviewExt>::SALES_CHANGE_REVIEWS;

/// 创建本域集合的幂等命名索引。
///
/// 逐条落地数据模型 §6.5「必需约束与索引」：
/// - 审批/复核「待处理按责任角色、创建时间」：责任角色即审批阶段
///   （销售领导/运营/低毛利上级，W05），索引建在 `status + review_stage +
///   created_at`；
/// - 采购二次确认「同一销售提交仅一个有效确认批次」使用**部分唯一索引**
///   （仅 `PENDING` 参与唯一）：已决策的批次永久保留为历史。回滚方式：
///   先删除该部分唯一索引、用应用层查重过渡，再由数据修复任务收敛数据后重建；
/// - 销售变更「同一销售单同一 `base_revision_id` 同时只能有一个进行中变更」使用
///   **部分唯一索引**（仅 `DRAFT`/`PENDING_IMPACT_CONFIRMATION`/
///   `PENDING_FINANCE_REVIEW`/`REJECTED` 参与唯一）：已生效/已作废变更保留为
///   历史。回滚方式同上；
/// - 审批/确认/变更提交是事实类集合，唯一约束直接建在身份组合上（无软删除语义）。
///
/// # 参数
/// * `db` - 目标 MongoDB 数据库
///
/// # 错误
/// 当已有数据违反唯一约束或 MongoDB 无法创建索引时返回错误。
pub(crate) async fn ensure(db: &Database) -> Result<()> {
    create_indexes(db, SALES_ORDER_REVIEWS, sales_order_review_indexes()).await?;
    create_indexes(db, PROCUREMENT_CONFIRMATIONS, procurement_confirmation_indexes()).await?;
    create_indexes(
        db,
        PROCUREMENT_CONFIRMATION_LINES,
        procurement_confirmation_line_indexes(),
    )
    .await?;
    create_indexes(db, SALES_CHANGE_ORDERS, sales_change_order_indexes()).await?;
    create_indexes(db, SALES_CHANGE_SUBMISSIONS, sales_change_submission_indexes()).await?;
    create_indexes(
        db,
        SALES_CHANGE_SUBMISSION_LINES,
        sales_change_submission_line_indexes(),
    )
    .await?;
    create_indexes(db, SALES_CHANGE_REVIEWS, sales_change_review_indexes()).await?;
    Ok(())
}

/// 为单个集合创建一组幂等命名索引。
async fn create_indexes(db: &Database, collection: &str, indexes: Vec<IndexModel>) -> Result<()> {
    db.collection::<Document>(collection)
        .create_indexes(indexes)
        .await?;
    Ok(())
}

/// 返回 `sales_order_review` 的审批唯一约束与待处理队列索引。
fn sales_order_review_indexes() -> Vec<IndexModel> {
    vec![
        unique_index(
            "uk_sales_order_reviews_submission_stage",
            doc! { "submission_id": 1, "review_stage": 1 },
        ),
        named_index(
            "idx_sales_order_reviews_pending_role_created",
            doc! { "status": 1, "review_stage": 1, "created_at": -1 },
        ),
    ]
}

/// 返回 `procurement_confirmation` 的有效批次唯一约束与待处理队列索引。
fn procurement_confirmation_indexes() -> Vec<IndexModel> {
    vec![
        unique_partial_index(
            "uk_procurement_confirmations_pending_per_submission",
            doc! { "submission_id": 1 },
            doc! { "status": "PENDING" },
        ),
        named_index(
            "idx_procurement_confirmations_pending_created",
            doc! { "status": 1, "created_at": -1 },
        ),
    ]
}

/// 返回 `procurement_confirmation_line` 的分行唯一约束。
fn procurement_confirmation_line_indexes() -> Vec<IndexModel> {
    vec![unique_index(
        "uk_procurement_confirmation_lines_confirmation_line",
        doc! { "procurement_confirmation_id": 1, "line_no": 1 },
    )]
}

/// 返回 `sales_change_order` 的进行中唯一约束与列表查询索引。
fn sales_change_order_indexes() -> Vec<IndexModel> {
    vec![
        unique_partial_index(
            "uk_sales_change_orders_active_per_order_base",
            doc! { "sales_order_id": 1, "base_revision_id": 1 },
            doc! {
                "$or": [
                    { "status": "DRAFT" },
                    { "status": "PENDING_IMPACT_CONFIRMATION" },
                    { "status": "PENDING_FINANCE_REVIEW" },
                    { "status": "REJECTED" },
                ]
            },
        ),
        named_index(
            "idx_sales_change_orders_order_status",
            doc! { "sales_order_id": 1, "status": 1 },
        ),
    ]
}

/// 返回 `sales_change_submission` 的身份约束与历史查询索引。
fn sales_change_submission_indexes() -> Vec<IndexModel> {
    vec![
        unique_index(
            "uk_sales_change_submissions_order_submission_no",
            doc! { "sales_change_order_id": 1, "submission_no": 1 },
        ),
        named_index(
            "idx_sales_change_submissions_order_submitted",
            doc! { "sales_change_order_id": 1, "submitted_at": -1 },
        ),
    ]
}

/// 返回 `sales_change_submission_line` 的明细唯一约束。
fn sales_change_submission_line_indexes() -> Vec<IndexModel> {
    vec![unique_index(
        "uk_sales_change_submission_lines_submission_line",
        doc! { "sales_change_submission_id": 1, "sales_order_line_id": 1 },
    )]
}

/// 返回 `sales_change_review` 的复核唯一约束与待处理队列索引。
fn sales_change_review_indexes() -> Vec<IndexModel> {
    vec![
        unique_index(
            "uk_sales_change_reviews_submission_stage",
            doc! { "sales_change_submission_id": 1, "review_stage": 1 },
        ),
        named_index(
            "idx_sales_change_reviews_pending_role_created",
            doc! { "status": 1, "review_stage": 1, "created_at": -1 },
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

/// 构建命名部分唯一索引。
fn unique_partial_index(name: impl Into<String>, keys: Document, filter: Document) -> IndexModel {
    IndexModel::builder()
        .keys(keys)
        .options(
            IndexOptions::builder()
                .name(name.into())
                .unique(true)
                .partial_filter_expression(filter)
                .build(),
        )
        .build()
}

#[cfg(test)]
mod tests {
    use mongodb::bson::doc;

    use super::{
        procurement_confirmation_indexes, sales_change_order_indexes, sales_change_review_indexes,
        sales_order_review_indexes,
    };

    #[test]
    fn sales_order_review_identity_index_is_unique() {
        let indexes = sales_order_review_indexes();

        let identity = indexes
            .iter()
            .find(|index| {
                index.options.as_ref().and_then(|options| options.name.as_deref())
                    == Some("uk_sales_order_reviews_submission_stage")
            })
            .unwrap();
        assert_eq!(identity.keys, doc! { "submission_id": 1, "review_stage": 1 });
        assert_eq!(identity.options.as_ref().unwrap().unique, Some(true));

        assert!(indexes
            .iter()
            .any(|index| { index.keys == doc! { "status": 1, "review_stage": 1, "created_at": -1 } }));
    }

    #[test]
    fn procurement_confirmation_pending_uniqueness_is_partial() {
        let indexes = procurement_confirmation_indexes();

        let pending = indexes
            .iter()
            .find(|index| {
                index.options.as_ref().and_then(|options| options.name.as_deref())
                    == Some("uk_procurement_confirmations_pending_per_submission")
            })
            .unwrap();
        assert_eq!(pending.keys, doc! { "submission_id": 1 });
        let options = pending.options.as_ref().unwrap();
        assert_eq!(options.unique, Some(true));
        assert_eq!(
            options.partial_filter_expression.as_ref().unwrap(),
            &doc! { "status": "PENDING" }
        );
    }

    #[test]
    fn sales_change_order_active_uniqueness_is_partial() {
        let indexes = sales_change_order_indexes();

        let active = indexes
            .iter()
            .find(|index| {
                index.options.as_ref().and_then(|options| options.name.as_deref())
                    == Some("uk_sales_change_orders_active_per_order_base")
            })
            .unwrap();
        assert_eq!(active.keys, doc! { "sales_order_id": 1, "base_revision_id": 1 });
        let options = active.options.as_ref().unwrap();
        assert_eq!(options.unique, Some(true));
        let filter = options.partial_filter_expression.as_ref().unwrap();
        assert!(filter.get_array("$or").is_ok(), "进行中状态参与唯一");
    }

    #[test]
    fn change_review_indexes_cover_submission_stage_and_pending_queue() {
        let indexes = sales_change_review_indexes();

        assert!(indexes.iter().any(|index| {
            index.options.as_ref().and_then(|options| options.name.as_deref())
                == Some("uk_sales_change_reviews_submission_stage")
                && index.options.as_ref().and_then(|options| options.unique) == Some(true)
        }));
        assert!(indexes
            .iter()
            .any(|index| { index.keys == doc! { "status": 1, "review_stage": 1, "created_at": -1 } }));
    }
}
