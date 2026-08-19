//! 域 D14 `sales_review` 的索引声明：sales_change_order、sales_change_submission(+_line)。

use mongodb::{
    bson::{doc, Document},
    options::IndexOptions,
    Database, IndexModel,
};

use crate::repository::extensions::SalesReviewExt;
use crate::Result;

/// `sales_change_order` 集合名。
pub(crate) const SALES_CHANGE_ORDERS: &str = <mongodb::Database as SalesReviewExt>::SALES_CHANGE_ORDERS;
/// `sales_change_submission` 集合名。
pub(crate) const SALES_CHANGE_SUBMISSIONS: &str =
    <mongodb::Database as SalesReviewExt>::SALES_CHANGE_SUBMISSIONS;
/// `sales_change_submission_line` 集合名。
pub(crate) const SALES_CHANGE_SUBMISSION_LINES: &str =
    <mongodb::Database as SalesReviewExt>::SALES_CHANGE_SUBMISSION_LINES;

/// 创建本域集合的幂等命名索引。
///
/// # 参数
/// * `db` - 目标 MongoDB 数据库
///
/// # 错误
/// 当已有数据违反唯一约束或 MongoDB 无法创建索引时返回错误。
pub(crate) async fn ensure(db: &Database) -> Result<()> {
    create_indexes(db, SALES_CHANGE_ORDERS, sales_change_order_indexes()).await?;
    create_indexes(db, SALES_CHANGE_SUBMISSIONS, sales_change_submission_indexes()).await?;
    create_indexes(
        db,
        SALES_CHANGE_SUBMISSION_LINES,
        sales_change_submission_line_indexes(),
    )
    .await?;
    Ok(())
}

/// 为单个集合创建一组幂等命名索引。
///
/// # 参数
/// * `db` - 目标 MongoDB 数据库
/// * `collection` - 集合名
/// * `indexes` - 待创建的命名索引
///
/// # 错误
/// 当已有数据违反唯一约束、同名索引定义冲突或 MongoDB 无法创建索引时返回错误。
async fn create_indexes(db: &Database, collection: &str, indexes: Vec<IndexModel>) -> Result<()> {
    db.collection::<Document>(collection)
        .create_indexes(indexes)
        .await?;
    Ok(())
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

    use super::sales_change_order_indexes;

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
}
