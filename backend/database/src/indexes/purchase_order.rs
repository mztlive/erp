//! 域 D15 `purchase_order` 的索引声明：purchase_order、purchase_order_submission(+line)、
//! purchase_order_revision(+line)、purchase_line_sales_allocation、purchase_change_order、
//! purchase_change_submission(+line)。
//!
//! 集合名常量取 `PurchaseOrderExt` 关联常量（唯一权威来源，conventions §4.3）：
//! `indexes/` 与 `repository/` 均为冻结声明下的私有子树，模块路径无法互相引用，
//! 关联常量随 trait 公开可达，两侧共用同一值，禁止字面量重复。
//!
//! 逐条落地数据模型 §6.6「必需约束与索引」：
//! - `purchase_no` 唯一、`(purchase_order_id, submission_no)` 唯一、
//!   `(purchase_order_id, revision_no)` 唯一、`(purchase_order_revision_id, line_no)` 唯一、
//!   `(purchase_order_revision_line_id, sales_order_revision_line_id)` 唯一、
//!   `(purchase_change_order_id, submission_no)` 唯一；
//! - 采购行/销售行双向查询索引；
//! - `supplier_id + status`、`sales_order_id + status` 查询索引；
//! - `(purchase_order_id, status, posted_at)` 查询索引（采购提交以 `submitted_at`
//!   承担冻结/过账时间轴，见 `purchase_order_submissions` 说明）。
//!
//! `purchase_no` 是身份类字段，采用**全局唯一索引**（与 accounts 的 code 处理一致）：
//! 软删除后仍保留单号，避免复用破坏单据追溯与恢复语义。提交/版本/分配是
//! 事实或修订类集合，不做软删除，无需在唯一索引上考虑删除态。

use mongodb::{
    bson::{doc, Document},
    options::IndexOptions,
    Database, IndexModel,
};

use crate::repository::extensions::PurchaseOrderExt;
use crate::Result;

/// `purchase_order` 集合名。
pub(crate) const PURCHASE_ORDERS: &str = <mongodb::Database as PurchaseOrderExt>::PURCHASE_ORDERS;
/// `purchase_order_submission` 集合名。
pub(crate) const PURCHASE_ORDER_SUBMISSIONS: &str =
    <mongodb::Database as PurchaseOrderExt>::PURCHASE_ORDER_SUBMISSIONS;
/// `purchase_order_submission_line` 集合名。
pub(crate) const PURCHASE_ORDER_SUBMISSION_LINES: &str =
    <mongodb::Database as PurchaseOrderExt>::PURCHASE_ORDER_SUBMISSION_LINES;
/// `purchase_order_revision` 集合名。
pub(crate) const PURCHASE_ORDER_REVISIONS: &str =
    <mongodb::Database as PurchaseOrderExt>::PURCHASE_ORDER_REVISIONS;
/// `purchase_order_revision_line` 集合名。
pub(crate) const PURCHASE_ORDER_REVISION_LINES: &str =
    <mongodb::Database as PurchaseOrderExt>::PURCHASE_ORDER_REVISION_LINES;
/// `purchase_line_sales_allocation` 集合名。
pub(crate) const PURCHASE_LINE_SALES_ALLOCATIONS: &str =
    <mongodb::Database as PurchaseOrderExt>::PURCHASE_LINE_SALES_ALLOCATIONS;
/// `purchase_change_order` 集合名。
pub(crate) const PURCHASE_CHANGE_ORDERS: &str =
    <mongodb::Database as PurchaseOrderExt>::PURCHASE_CHANGE_ORDERS;
/// `purchase_change_submission` 集合名。
pub(crate) const PURCHASE_CHANGE_SUBMISSIONS: &str =
    <mongodb::Database as PurchaseOrderExt>::PURCHASE_CHANGE_SUBMISSIONS;
/// `purchase_change_submission_line` 集合名。
pub(crate) const PURCHASE_CHANGE_SUBMISSION_LINES: &str =
    <mongodb::Database as PurchaseOrderExt>::PURCHASE_CHANGE_SUBMISSION_LINES;

/// 创建本域集合的幂等命名索引。
///
/// 逐条落地数据模型 §6.6「必需约束与索引」；唯一约束一律用唯一索引表达。
///
/// # 参数
/// * `db` - 目标 MongoDB 数据库
///
/// # 错误
/// 当已有数据违反唯一约束或 MongoDB 无法创建索引时返回错误。
pub(crate) async fn ensure(db: &Database) -> Result<()> {
    create_indexes(db, PURCHASE_ORDERS, purchase_order_indexes()).await?;
    create_indexes(
        db,
        PURCHASE_ORDER_SUBMISSIONS,
        purchase_order_submission_indexes(),
    )
    .await?;
    create_indexes(
        db,
        PURCHASE_ORDER_SUBMISSION_LINES,
        purchase_order_submission_line_indexes(),
    )
    .await?;
    create_indexes(db, PURCHASE_ORDER_REVISIONS, purchase_order_revision_indexes()).await?;
    create_indexes(
        db,
        PURCHASE_ORDER_REVISION_LINES,
        purchase_order_revision_line_indexes(),
    )
    .await?;
    create_indexes(
        db,
        PURCHASE_LINE_SALES_ALLOCATIONS,
        purchase_line_sales_allocation_indexes(),
    )
    .await?;
    create_indexes(db, PURCHASE_CHANGE_ORDERS, purchase_change_order_indexes()).await?;
    create_indexes(
        db,
        PURCHASE_CHANGE_SUBMISSIONS,
        purchase_change_submission_indexes(),
    )
    .await?;
    create_indexes(
        db,
        PURCHASE_CHANGE_SUBMISSION_LINES,
        purchase_change_submission_line_indexes(),
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

/// 返回 `purchase_order` 的单号约束与列表查询索引。
///
/// `purchase_no` 全局唯一（软删除后仍保留单号）；`supplier_id + status` 与
/// `sales_order_id + status` 是 §6.6 查询索引的前缀形态（`expected_date` 不在
/// 采购主表实体字段内，W08 中「最近预计交期」由版本行服务端汇总派生，
/// 无法直接建索引，见 P2 报告）。
fn purchase_order_indexes() -> Vec<IndexModel> {
    vec![
        unique_index("uk_purchase_orders_purchase_no", doc! { "purchase_no": 1 }),
        named_index(
            "idx_purchase_orders_supplier_status",
            doc! { "supplier_id": 1, "status": 1 },
        ),
        named_index(
            "idx_purchase_orders_sales_status",
            doc! { "sales_order_id": 1, "status": 1 },
        ),
    ]
}

/// 返回 `purchase_order_submission` 的提交序号约束与审核队列索引。
///
/// `(purchase_order_id, submission_no)` 唯一（§6.6）；`(purchase_order_id, status,
/// submitted_at)` 承担任务书要求的 `(purchase_order_id, status, posted_at)` 查询
/// 索引——采购域没有 `posted_at` 字段，提交冻结进入待审核的时间轴由
/// `submitted_at` 表达，财务审核队列按（采购单, 状态, 提交时间）过滤。
fn purchase_order_submission_indexes() -> Vec<IndexModel> {
    vec![
        unique_index(
            "uk_purchase_order_submissions_order_no",
            doc! { "purchase_order_id": 1, "submission_no": 1 },
        ),
        named_index(
            "idx_purchase_order_submissions_order_status_posted",
            doc! { "purchase_order_id": 1, "status": 1, "submitted_at": 1 },
        ),
    ]
}

/// 返回 `purchase_order_submission_line` 的行号唯一约束。
fn purchase_order_submission_line_indexes() -> Vec<IndexModel> {
    vec![unique_index(
        "uk_purchase_order_submission_lines_order_line",
        doc! { "purchase_order_submission_id": 1, "line_no": 1 },
    )]
}

/// 返回 `purchase_order_revision` 的版本号唯一约束。
fn purchase_order_revision_indexes() -> Vec<IndexModel> {
    vec![unique_index(
        "uk_purchase_order_revisions_order_no",
        doc! { "purchase_order_id": 1, "revision_no": 1 },
    )]
}

/// 返回 `purchase_order_revision_line` 的版本内行号唯一约束（§6.6）。
fn purchase_order_revision_line_indexes() -> Vec<IndexModel> {
    vec![unique_index(
        "uk_purchase_order_revision_lines_revision_line",
        doc! { "purchase_order_revision_id": 1, "line_no": 1 },
    )]
}

/// 返回 `purchase_line_sales_allocation` 的分配唯一约束与双向查询索引（§6.6）。
///
/// 唯一索引承载正向查询（按采购版本行）；反向索引 `sales_order_revision_line_id`
/// 在前，供「被满足的销售明细 → 采购分配」查询与入库预占回源使用。
fn purchase_line_sales_allocation_indexes() -> Vec<IndexModel> {
    vec![
        unique_index(
            "uk_purchase_line_sales_allocations_link",
            doc! {
                "purchase_order_revision_line_id": 1,
                "sales_order_revision_line_id": 1,
            },
        ),
        named_index(
            "idx_purchase_line_sales_allocations_sales_line",
            doc! {
                "sales_order_revision_line_id": 1,
                "purchase_order_revision_line_id": 1,
            },
        ),
    ]
}

/// 返回 `purchase_change_order` 的变更历史查询索引。
fn purchase_change_order_indexes() -> Vec<IndexModel> {
    vec![named_index(
        "idx_purchase_change_orders_order_status",
        doc! { "purchase_order_id": 1, "status": 1 },
    )]
}

/// 返回 `purchase_change_submission` 的提交序号唯一约束（§6.6）。
fn purchase_change_submission_indexes() -> Vec<IndexModel> {
    vec![unique_index(
        "uk_purchase_change_submissions_order_no",
        doc! { "purchase_change_order_id": 1, "submission_no": 1 },
    )]
}

/// 返回 `purchase_change_submission_line` 的变更提交内行号唯一约束。
fn purchase_change_submission_line_indexes() -> Vec<IndexModel> {
    vec![unique_index(
        "uk_purchase_change_submission_lines_order_line",
        doc! { "purchase_change_submission_id": 1, "line_no": 1 },
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
        purchase_change_submission_indexes, purchase_line_sales_allocation_indexes, purchase_order_indexes,
        purchase_order_revision_indexes, purchase_order_submission_indexes, unique_index,
    };

    #[test]
    fn purchase_order_no_is_globally_unique_and_query_indexes_exist() {
        let indexes = purchase_order_indexes();

        let no_index = indexes
            .iter()
            .find(|index| {
                index.options.as_ref().and_then(|options| options.name.as_deref())
                    == Some("uk_purchase_orders_purchase_no")
            })
            .unwrap();
        assert_eq!(no_index.keys, doc! { "purchase_no": 1 });
        assert_eq!(no_index.options.as_ref().unwrap().unique, Some(true));

        assert!(indexes
            .iter()
            .any(|index| index.keys == doc! { "supplier_id": 1, "status": 1 }));
        assert!(indexes
            .iter()
            .any(|index| index.keys == doc! { "sales_order_id": 1, "status": 1 }));
    }

    #[test]
    fn submission_order_no_unique_and_queue_index_carries_submitted_at() {
        let indexes = purchase_order_submission_indexes();

        assert!(indexes.iter().any(|index| {
            index.options.as_ref().and_then(|options| options.name.as_deref())
                == Some("uk_purchase_order_submissions_order_no")
                && index.options.as_ref().and_then(|options| options.unique) == Some(true)
        }));
        assert!(indexes.iter().any(|index| {
            index.keys
                == doc! {
                    "purchase_order_id": 1,
                    "status": 1,
                    "submitted_at": 1,
                }
        }));
    }

    #[test]
    fn revision_and_allocation_indexes_cover_unique_keys_and_bidirectional_query() {
        let revision_indexes = purchase_order_revision_indexes();
        assert!(revision_indexes.iter().any(|index| {
            index.options.as_ref().and_then(|options| options.name.as_deref())
                == Some("uk_purchase_order_revisions_order_no")
                && index.options.as_ref().and_then(|options| options.unique) == Some(true)
        }));

        let allocation_indexes = purchase_line_sales_allocation_indexes();
        assert!(allocation_indexes.iter().any(|index| {
            index.options.as_ref().and_then(|options| options.name.as_deref())
                == Some("uk_purchase_line_sales_allocations_link")
                && index.options.as_ref().and_then(|options| options.unique) == Some(true)
        }));
        assert!(allocation_indexes.iter().any(|index| {
            index.keys
                == doc! {
                    "sales_order_revision_line_id": 1,
                    "purchase_order_revision_line_id": 1,
                }
        }));
    }

    #[test]
    fn change_submission_order_no_unique() {
        let indexes = purchase_change_submission_indexes();

        assert!(indexes.iter().any(|index| {
            index.options.as_ref().and_then(|options| options.name.as_deref())
                == Some("uk_purchase_change_submissions_order_no")
                && index.options.as_ref().and_then(|options| options.unique) == Some(true)
        }));
        assert_eq!(
            unique_index("probe", doc! { "a": 1 })
                .options
                .as_ref()
                .unwrap()
                .unique,
            Some(true)
        );
    }
}
