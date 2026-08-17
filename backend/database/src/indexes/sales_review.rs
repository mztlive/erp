//! 域 D14 `sales_review` 的索引声明：sales_order_review、procurement_confirmation
//! (+_line)、sales_change_order、sales_change_submission(+_line)、sales_change_review
//! （页面：W05、W07）。
//!
//! 集合名常量取 `SalesReviewExt` 关联常量（唯一权威来源，conventions §4.3）：
//! `indexes/` 与 `repository/` 均为冻结声明下的私有子树，模块路径无法互相引用，
//! 关联常量随 trait 公开可达，两侧共用同一值，禁止字面量重复。

use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use mongodb::{
    bson::{doc, Document},
    options::IndexOptions,
    Database, IndexModel,
};

use crate::repository::extensions::SalesReviewExt;
use crate::Result;

/// `sales_order_review` 集合名。
pub(crate) const SALES_ORDER_REVIEWS: &str = <mongodb::Database as SalesReviewExt>::SALES_ORDER_REVIEWS;
/// `low_margin_manager_confirmation` 集合名。
pub(crate) const LOW_MARGIN_MANAGER_CONFIRMATIONS: &str =
    <mongodb::Database as SalesReviewExt>::LOW_MARGIN_MANAGER_CONFIRMATIONS;
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
/// - `sales_order_review` 只保存正式决定，历史按决定、阶段与审批时间查询；审批
///   等待和当前步骤由 D03 审批实例承担；
/// - 采购二次确认「同一销售提交仅一个有效确认批次」使用**部分唯一索引**
///   （仅 `PENDING` 参与唯一）：已决策的批次永久保留为历史。回滚方式：
///   先删除该部分唯一索引、用应用层查重过渡，再由数据修复任务收敛数据后重建；
/// - 销售变更「同一销售单同一 `base_revision_id` 同时只能有一个进行中变更」使用
///   **部分唯一索引**（仅 `DRAFT`/`PENDING_IMPACT_CONFIRMATION`/
///   `PENDING_FINANCE_REVIEW`/`REJECTED` 参与唯一）：已生效/已作废变更保留为
///   历史。回滚方式同上；
/// - 采购确认分行是可被反复替换的工作数据：保存时先软删旧行再写入新行。
///   `(procurement_confirmation_id, line_no)` 唯一只约束未删除行；
///   全量唯一索引会让第二次保存撞上已软删的同号行。回滚方式：删除部分唯一
///   索引、恢复旧的全量唯一索引名，并停止对已删除行复用 `line_no`；
/// - 审批/确认/变更提交是事实类集合，唯一约束直接建在身份组合上（无软删除语义）。
///
/// # 参数
/// * `db` - 目标 MongoDB 数据库
///
/// # 错误
/// 当已有数据违反唯一约束或 MongoDB 无法创建索引时返回错误。
pub(crate) async fn ensure(db: &Database) -> Result<()> {
    create_indexes(db, SALES_ORDER_REVIEWS, sales_order_review_indexes()).await?;
    create_indexes(
        db,
        LOW_MARGIN_MANAGER_CONFIRMATIONS,
        low_margin_manager_confirmation_indexes(),
    )
    .await?;
    create_indexes(db, PROCUREMENT_CONFIRMATIONS, procurement_confirmation_indexes()).await?;
    drop_named_index_if_exists(
        db,
        PROCUREMENT_CONFIRMATION_LINES,
        LEGACY_PROCUREMENT_CONFIRMATION_LINE_UNIQUE_INDEX,
    )
    .await?;
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

/// 返回低毛利申请按新提交唯一、按销售单查询的索引。
fn low_margin_manager_confirmation_indexes() -> Vec<IndexModel> {
    vec![
        unique_index(
            "uk_low_margin_confirmations_submission",
            doc! { "low_margin_submission_id": 1 },
        ),
        named_index(
            "idx_low_margin_confirmations_order_status_created",
            doc! { "sales_order_id": 1, "status": 1, "created_at": -1 },
        ),
    ]
}

/// 为单个集合创建一组幂等命名索引。
///
/// # 参数
/// * `db` - 目标 MongoDB 数据库
/// * `collection` - 集合名
/// * `indexes` - 待创建的命名索引
///
/// # 返回
/// 索引已存在且定义一致时幂等成功。
///
/// # 错误
/// 当已有数据违反唯一约束、同名索引定义冲突或 MongoDB 无法创建索引时返回错误。
async fn create_indexes(db: &Database, collection: &str, indexes: Vec<IndexModel>) -> Result<()> {
    db.collection::<Document>(collection)
        .create_indexes(indexes)
        .await?;
    Ok(())
}

/// 按名称删除索引；索引不存在时视为已完成。
///
/// # 参数
/// * `db` - 目标 MongoDB 数据库
/// * `collection` - 集合名
/// * `name` - 要删除的索引名
///
/// # 返回
/// 索引已删除或不存在时返回成功。
///
/// # 错误
/// 列举或删除索引失败且原因不是“索引不存在”时返回错误。
///
/// # 约束
/// 只用于把旧的全量唯一索引替换为部分唯一索引，不得用于删除业务数据。
async fn drop_named_index_if_exists(db: &Database, collection: &str, name: &str) -> Result<()> {
    let names = db.collection::<Document>(collection).list_index_names().await?;
    if !names.iter().any(|existing| existing == name) {
        return Ok(());
    }
    db.collection::<Document>(collection).drop_index(name).await?;
    Ok(())
}

/// 返回 `sales_order_review` 的审批决定唯一约束与历史查询索引。
fn sales_order_review_indexes() -> Vec<IndexModel> {
    vec![
        unique_index(
            "uk_sales_order_reviews_submission_stage",
            doc! { "submission_id": 1, "review_stage": 1 },
        ),
        named_index(
            "idx_sales_order_reviews_decision_stage_reviewed",
            doc! { "status": 1, "review_stage": 1, "reviewed_at": -1 },
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

/// 旧的全量唯一索引名。保存会软删后再写入同号分行，该索引会把已删除行算进唯一。
const LEGACY_PROCUREMENT_CONFIRMATION_LINE_UNIQUE_INDEX: &str =
    "uk_procurement_confirmation_lines_confirmation_line";

/// 未删除采购确认分行的部分唯一索引名。
const PROCUREMENT_CONFIRMATION_LINE_ACTIVE_UNIQUE_INDEX: &str =
    "uk_procurement_confirmation_lines_active_confirmation_line";

/// 返回 `procurement_confirmation_line` 的未删除分行唯一约束。
///
/// # 参数
/// 无。
///
/// # 返回
/// 返回只约束 `deleted_at = 0` 行的部分唯一索引。
///
/// # 错误
/// 无。
///
/// # 约束
/// 保存采购确认工作数据会软删旧分行并插入相同 `line_no` 的新分行。
fn procurement_confirmation_line_indexes() -> Vec<IndexModel> {
    vec![unique_partial_index(
        PROCUREMENT_CONFIRMATION_LINE_ACTIVE_UNIQUE_INDEX,
        doc! { "procurement_confirmation_id": 1, "line_no": 1 },
        doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON },
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
        low_margin_manager_confirmation_indexes, procurement_confirmation_indexes,
        procurement_confirmation_line_indexes, sales_change_order_indexes, sales_change_review_indexes,
        sales_order_review_indexes, PROCUREMENT_CONFIRMATION_LINE_ACTIVE_UNIQUE_INDEX,
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
            .any(|index| { index.keys == doc! { "status": 1, "review_stage": 1, "reviewed_at": -1 } }));
    }

    #[test]
    fn procurement_confirmation_line_uniqueness_ignores_soft_deleted_rows() {
        let indexes = procurement_confirmation_line_indexes();

        let active = indexes
            .iter()
            .find(|index| {
                index.options.as_ref().and_then(|options| options.name.as_deref())
                    == Some(PROCUREMENT_CONFIRMATION_LINE_ACTIVE_UNIQUE_INDEX)
            })
            .unwrap();
        assert_eq!(
            active.keys,
            doc! { "procurement_confirmation_id": 1, "line_no": 1 }
        );
        let options = active.options.as_ref().unwrap();
        assert_eq!(options.unique, Some(true));
        assert_eq!(
            options.partial_filter_expression.as_ref().unwrap(),
            &doc! { "deleted_at": entity_core::NOT_DELETED_TIMESTAMP_BSON }
        );
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
    fn low_margin_confirmation_is_permanently_unique_per_submission() {
        let indexes = low_margin_manager_confirmation_indexes();
        let identity = indexes
            .iter()
            .find(|index| {
                index.options.as_ref().and_then(|options| options.name.as_deref())
                    == Some("uk_low_margin_confirmations_submission")
            })
            .unwrap();
        assert_eq!(identity.keys, doc! { "low_margin_submission_id": 1 });
        let options = identity.options.as_ref().unwrap();
        assert_eq!(options.unique, Some(true));
        assert!(options.partial_filter_expression.is_none());
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
