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
//! `purchase_no` 是身份类字段，首次提交前为空；仅非空正式号进入全局唯一索引，
//! 允许多张未编号草稿并存。软删除后正式号仍参与唯一约束，避免复用破坏单据
//! 追溯与恢复语义。提交/版本/分配是事实或修订类集合，不做软删除。

use futures_util::TryStreamExt;

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
    reconcile_purchase_order_no_index(db).await?;
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

/// 将旧的全量采购单号唯一索引升级为仅约束非空正式号的部分唯一索引。
///
/// 草稿的 `purchase_no` 固定为空字符串；旧索引会让第二张草稿触发重复键。
/// 仅在同名索引契约不匹配时删除，随后由 `create_indexes` 重建目标索引。
async fn reconcile_purchase_order_no_index(db: &Database) -> Result<()> {
    let collection_names = db.list_collection_names().await?;
    if !collection_names.iter().any(|name| name == PURCHASE_ORDERS) {
        return Ok(());
    }

    let collection = db.collection::<Document>(PURCHASE_ORDERS);
    let mut indexes = collection.list_indexes().await?;
    while let Some(index) = indexes.try_next().await? {
        let name = index.options.as_ref().and_then(|options| options.name.as_deref());
        if name == Some("uk_purchase_orders_purchase_no") && !is_current_purchase_no_index(&index) {
            collection.drop_index("uk_purchase_orders_purchase_no").await?;
            break;
        }
    }
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
/// 非空 `purchase_no` 全局唯一（草稿空号不进入索引，软删除正式号仍保留）；
/// `supplier_id + status` 与 `sales_order_id + status` 是 §6.6 查询索引的前缀形态
/// （`expected_date` 不在采购主表实体字段内，W08 中「最近预计交期」由版本行
/// 服务端汇总派生，无法直接建索引，见 P2 报告）。
fn purchase_order_indexes() -> Vec<IndexModel> {
    vec![
        unique_partial_index(
            "uk_purchase_orders_purchase_no",
            doc! { "purchase_no": 1 },
            formal_purchase_no_filter(),
        ),
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

/// 正式采购号唯一约束的命中条件；空字符串草稿不参与唯一性。
fn formal_purchase_no_filter() -> Document {
    doc! { "purchase_no": { "$type": "string", "$gt": "" } }
}

/// 判断服务端已有索引是否符合当前采购号约束。
fn is_current_purchase_no_index(index: &IndexModel) -> bool {
    let Some(options) = index.options.as_ref() else {
        return false;
    };
    index.keys == doc! { "purchase_no": 1 }
        && options.unique == Some(true)
        && options.partial_filter_expression.as_ref() == Some(&formal_purchase_no_filter())
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
        formal_purchase_no_filter, is_current_purchase_no_index, purchase_change_submission_indexes,
        purchase_line_sales_allocation_indexes, purchase_order_indexes, purchase_order_revision_indexes,
        purchase_order_submission_indexes, unique_index,
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
        assert_eq!(
            no_index.options.as_ref().unwrap().partial_filter_expression,
            Some(formal_purchase_no_filter())
        );
        assert!(is_current_purchase_no_index(no_index));

        let legacy = unique_index("uk_purchase_orders_purchase_no", doc! { "purchase_no": 1 });
        assert!(!is_current_purchase_no_index(&legacy));

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
