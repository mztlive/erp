//! 域 D13 `sales_order` 的索引声明：sales_order(+_line)、工作副本、提交、版本
//! 及两个版本行子类型（页面：W05）。
//!
//! 集合名常量取 `SalesOrderExt` 关联常量（唯一权威来源，conventions §4.3）：
//! `indexes/` 与 `repository/` 均为冻结声明下的私有子树，模块路径无法互相引用，
//! 关联常量随 trait 公开可达，两侧共用同一值，禁止字面量重复。

use mongodb::{
    bson::{doc, Document},
    options::IndexOptions,
    Database, IndexModel,
};

use crate::repository::extensions::SalesOrderExt;
use crate::Result;

/// `sales_order` 集合名。
pub(crate) const SALES_ORDERS: &str = <mongodb::Database as SalesOrderExt>::SALES_ORDERS;
/// `sales_order_line` 集合名。
pub(crate) const SALES_ORDER_LINES: &str = <mongodb::Database as SalesOrderExt>::SALES_ORDER_LINES;
/// `sales_order_working_copy` 集合名。
pub(crate) const SALES_ORDER_WORKING_COPIES: &str =
    <mongodb::Database as SalesOrderExt>::SALES_ORDER_WORKING_COPIES;
/// `sales_order_working_copy_line` 集合名。
pub(crate) const SALES_ORDER_WORKING_COPY_LINES: &str =
    <mongodb::Database as SalesOrderExt>::SALES_ORDER_WORKING_COPY_LINES;
/// `sales_order_submission` 集合名。
pub(crate) const SALES_ORDER_SUBMISSIONS: &str =
    <mongodb::Database as SalesOrderExt>::SALES_ORDER_SUBMISSIONS;
/// `sales_order_submission_line` 集合名。
pub(crate) const SALES_ORDER_SUBMISSION_LINES: &str =
    <mongodb::Database as SalesOrderExt>::SALES_ORDER_SUBMISSION_LINES;
/// `sales_order_revision` 集合名。
pub(crate) const SALES_ORDER_REVISIONS: &str = <mongodb::Database as SalesOrderExt>::SALES_ORDER_REVISIONS;
/// `sales_order_revision_line` 集合名。
pub(crate) const SALES_ORDER_REVISION_LINES: &str =
    <mongodb::Database as SalesOrderExt>::SALES_ORDER_REVISION_LINES;
/// `sales_order_goods_service_line_revision` 集合名。
pub(crate) const SALES_ORDER_GOODS_SERVICE_LINE_REVISIONS: &str =
    <mongodb::Database as SalesOrderExt>::SALES_ORDER_GOODS_SERVICE_LINE_REVISIONS;
/// `sales_order_voucher_line_revision` 集合名。
pub(crate) const SALES_ORDER_VOUCHER_LINE_REVISIONS: &str =
    <mongodb::Database as SalesOrderExt>::SALES_ORDER_VOUCHER_LINE_REVISIONS;

/// 创建本域集合的幂等命名索引。
///
/// 逐条落地数据模型 §6.4/§6.5「必需约束与索引」：
/// - `order_no` 是身份类字段，使用**全局唯一索引**（与 accounts 的 code 处理
///   一致）：软删除后仍保留单号，避免复用破坏恢复语义；
/// - 工作副本「同一销售单和编辑目的同时最多一个有效工作副本」使用**部分唯一
///   索引**（仅 `EDITING`/`CONFLICT` 状态参与唯一）：历史草稿以
///   `SUBMITTED`/`ABANDONED` 状态永久保留，全局唯一会阻塞后续提交。回滚方式：
///   先删除该部分唯一索引、用应用层查重过渡，再由数据修复任务收敛数据后重建；
/// - 提交/版本是事实类集合，全部唯一约束直接建在身份组合上（无软删除语义）；
/// - `(sales_order_id, content_hash)` 用于幂等与历史查询，但同一内容可能被合法
///   重建，故为普通索引而非唯一索引（幂等判定由 P3 结合来源快照完成）；
/// - 数据模型「负责人参与人 + 状态」「履约期限」索引：负责人字段不落在
///   `sales_order` 主表实体上（归属后续批次），本域以审核轨
///   `review_status + created_at` 覆盖审核列表，履约期限按版本行落地
///   `idx_sales_order_revision_lines_due`。
///
/// # 参数
/// * `db` - 目标 MongoDB 数据库
///
/// # 错误
/// 当已有数据违反唯一约束或 MongoDB 无法创建索引时返回错误。
pub(crate) async fn ensure(db: &Database) -> Result<()> {
    create_indexes(db, SALES_ORDERS, sales_order_indexes()).await?;
    create_indexes(db, SALES_ORDER_LINES, sales_order_line_indexes()).await?;
    create_indexes(db, SALES_ORDER_WORKING_COPIES, working_copy_indexes()).await?;
    create_indexes(db, SALES_ORDER_WORKING_COPY_LINES, working_copy_line_indexes()).await?;
    create_indexes(db, SALES_ORDER_SUBMISSIONS, submission_indexes()).await?;
    create_indexes(db, SALES_ORDER_SUBMISSION_LINES, submission_line_indexes()).await?;
    create_indexes(db, SALES_ORDER_REVISIONS, revision_indexes()).await?;
    create_indexes(db, SALES_ORDER_REVISION_LINES, revision_line_indexes()).await?;
    create_indexes(
        db,
        SALES_ORDER_GOODS_SERVICE_LINE_REVISIONS,
        goods_service_line_revision_indexes(),
    )
    .await?;
    create_indexes(
        db,
        SALES_ORDER_VOUCHER_LINE_REVISIONS,
        voucher_line_revision_indexes(),
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

/// 返回 `sales_order` 的身份约束与业务列表查询索引。
fn sales_order_indexes() -> Vec<IndexModel> {
    vec![
        unique_index("uk_sales_orders_order_no", doc! { "order_no": 1 }),
        named_index(
            "idx_sales_orders_customer_status_created",
            doc! { "customer_id": 1, "commercial_status": 1, "created_at": -1 },
        ),
        named_index(
            "idx_sales_orders_review_status_created",
            doc! { "review_status": 1, "created_at": -1 },
        ),
    ]
}

/// 返回 `sales_order_line` 的稳定明细唯一约束。
fn sales_order_line_indexes() -> Vec<IndexModel> {
    vec![unique_index(
        "uk_sales_order_lines_order_line",
        doc! { "sales_order_id": 1, "line_no": 1 },
    )]
}

/// 返回 `sales_order_working_copy` 的有效唯一约束与列表查询索引。
fn working_copy_indexes() -> Vec<IndexModel> {
    vec![
        unique_partial_index(
            "uk_sales_order_working_copies_active_per_purpose",
            doc! { "sales_order_id": 1, "working_purpose": 1 },
            doc! {
                "$or": [
                    { "status": "EDITING" },
                    { "status": "CONFLICT" },
                ]
            },
        ),
        named_index(
            "idx_sales_order_working_copies_order_purpose",
            doc! { "sales_order_id": 1, "working_purpose": 1, "updated_at": -1 },
        ),
        named_index(
            "idx_sales_order_working_copies_status_updated",
            doc! { "status": 1, "updated_at": -1 },
        ),
    ]
}

/// 返回 `sales_order_working_copy_line` 的明细唯一约束。
fn working_copy_line_indexes() -> Vec<IndexModel> {
    vec![unique_index(
        "uk_sales_order_working_copy_lines_copy_line",
        doc! { "working_copy_id": 1, "sales_order_line_id": 1 },
    )]
}

/// 返回 `sales_order_submission` 的身份约束与历史查询索引。
fn submission_indexes() -> Vec<IndexModel> {
    vec![
        unique_index(
            "uk_sales_order_submissions_order_submission_no",
            doc! { "sales_order_id": 1, "submission_no": 1 },
        ),
        named_index(
            "idx_sales_order_submissions_order_submitted",
            doc! { "sales_order_id": 1, "submitted_at": -1 },
        ),
        named_index(
            "idx_sales_order_submissions_status_created",
            doc! { "status": 1, "created_at": -1 },
        ),
    ]
}

/// 返回 `sales_order_submission_line` 的明细唯一约束。
fn submission_line_indexes() -> Vec<IndexModel> {
    vec![unique_index(
        "uk_sales_order_submission_lines_submission_line",
        doc! { "submission_id": 1, "sales_order_line_id": 1 },
    )]
}

/// 返回 `sales_order_revision` 的身份约束与历史/幂等查询索引。
fn revision_indexes() -> Vec<IndexModel> {
    vec![
        unique_index(
            "uk_sales_order_revisions_order_revision_no",
            doc! { "sales_order_id": 1, "revision_no": 1 },
        ),
        named_index(
            "idx_sales_order_revisions_order_content_hash",
            doc! { "sales_order_id": 1, "content_hash": 1 },
        ),
        named_index(
            "idx_sales_order_revisions_order_effective",
            doc! { "sales_order_id": 1, "effective_at": -1 },
        ),
    ]
}

/// 返回 `sales_order_revision_line` 的公共行唯一约束与履约期限索引。
fn revision_line_indexes() -> Vec<IndexModel> {
    vec![
        unique_index(
            "uk_sales_order_revision_lines_revision_line",
            doc! { "sales_order_revision_id": 1, "sales_order_line_id": 1 },
        ),
        named_index(
            "idx_sales_order_revision_lines_due",
            doc! { "sales_order_revision_id": 1, "line_no": 1 },
        ),
    ]
}

/// 返回 `sales_order_goods_service_line_revision` 的一对一唯一约束。
fn goods_service_line_revision_indexes() -> Vec<IndexModel> {
    vec![unique_index(
        "uk_sales_order_goods_service_line_revisions_line",
        doc! { "revision_line_id": 1 },
    )]
}

/// 返回 `sales_order_voucher_line_revision` 的一对一唯一约束。
fn voucher_line_revision_indexes() -> Vec<IndexModel> {
    vec![unique_index(
        "uk_sales_order_voucher_line_revisions_line",
        doc! { "revision_line_id": 1 },
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
        goods_service_line_revision_indexes, revision_indexes, sales_order_indexes, sales_order_line_indexes,
        submission_indexes, voucher_line_revision_indexes, working_copy_indexes,
    };

    #[test]
    fn sales_order_identity_index_is_globally_unique() {
        let indexes = sales_order_indexes();

        let no_index = indexes
            .iter()
            .find(|index| {
                index.options.as_ref().and_then(|options| options.name.as_deref())
                    == Some("uk_sales_orders_order_no")
            })
            .unwrap();
        assert_eq!(no_index.keys, doc! { "order_no": 1 });
        assert_eq!(no_index.options.as_ref().unwrap().unique, Some(true));
        assert!(no_index
            .options
            .as_ref()
            .unwrap()
            .partial_filter_expression
            .is_none());

        assert!(indexes.iter().any(|index| {
            index.keys == doc! { "customer_id": 1, "commercial_status": 1, "created_at": -1 }
        }));
    }

    #[test]
    fn working_copy_active_uniqueness_is_partial() {
        let indexes = working_copy_indexes();

        let active = indexes
            .iter()
            .find(|index| {
                index.options.as_ref().and_then(|options| options.name.as_deref())
                    == Some("uk_sales_order_working_copies_active_per_purpose")
            })
            .unwrap();
        assert_eq!(active.keys, doc! { "sales_order_id": 1, "working_purpose": 1 });
        let options = active.options.as_ref().unwrap();
        assert_eq!(options.unique, Some(true));
        let filter = options.partial_filter_expression.as_ref().unwrap();
        assert!(filter.get_array("$or").is_ok(), "部分唯一索引按有效状态过滤");
    }

    #[test]
    fn line_and_submission_and_revision_uniquenesses_are_compound() {
        assert!(sales_order_line_indexes()
            .iter()
            .any(|index| index.keys == doc! { "sales_order_id": 1, "line_no": 1 }));
        assert!(submission_indexes().iter().any(|index| {
            index.keys == doc! { "sales_order_id": 1, "submission_no": 1 }
                && index.options.as_ref().and_then(|options| options.unique) == Some(true)
        }));
        assert!(revision_indexes().iter().any(|index| {
            index.keys == doc! { "sales_order_id": 1, "revision_no": 1 }
                && index.options.as_ref().and_then(|options| options.unique) == Some(true)
        }));
    }

    #[test]
    fn revision_content_hash_index_is_non_unique() {
        let indexes = revision_indexes();

        let content = indexes
            .iter()
            .find(|index| {
                index.options.as_ref().and_then(|options| options.name.as_deref())
                    == Some("idx_sales_order_revisions_order_content_hash")
            })
            .unwrap();
        assert_eq!(content.keys, doc! { "sales_order_id": 1, "content_hash": 1 });
        assert_ne!(content.options.as_ref().unwrap().unique, Some(true));
    }

    #[test]
    fn subtype_line_revisions_are_unique_by_public_line() {
        assert!(goods_service_line_revision_indexes()
            .iter()
            .any(|index| index.keys == doc! { "revision_line_id": 1 }));
        assert!(voucher_line_revision_indexes()
            .iter()
            .any(|index| index.keys == doc! { "revision_line_id": 1 }));
    }
}
