//! 域 D30 `mall_after_sales` 的索引声明：mall_after_sales_request(+_line)、
//! mall_refund(+_line)、mall_refund_allocation、mall_balance_restoration(+_allocation)。
//!
//! 集合名常量取 `MallAfterSalesExt` 关联常量（唯一权威来源，conventions §4.3）：
//! `indexes/` 与 `repository/` 均为冻结声明下的私有子树，模块路径无法互相引用，
//! 关联常量随 trait 公开可达，两侧共用同一值，禁止字面量重复。
//!
//! §6.18 逐条对照：
//! - `mall_after_sales_request`：「`(mall_id, external_request_id)` 唯一」→
//!   `uk_mall_after_sales_requests_identity`；「`mall_order_id + status` 查询索引」→
//!   `idx_mall_after_sales_requests_order_status`；
//! - `mall_after_sales_request_line`：「`(after_sales_request_id, line_no)` 及
//!   `(after_sales_request_id, mall_order_item_id)` 唯一」→
//!   `uk_mall_after_sales_request_lines_no`、`uk_mall_after_sales_request_lines_item`；
//!   「`mall_order_item_id + line_status` 查询索引」→
//!   `idx_mall_after_sales_request_lines_item_status`；
//! - `mall_refund`：「`mall_order_fact_id` 非空且唯一」→ `uk_mall_refunds_fact`；
//!   「商城 + 退款单号 + 退款版本唯一」→ `uk_mall_refunds_identity`；按售后案件
//!   回溯 → `idx_mall_refunds_after_sales_request`；
//! - `mall_refund_line`：「`(mall_refund_id, line_no)` 以及
//!   `(mall_refund_id, mall_order_item_id)` 唯一」→
//!   `uk_mall_refund_lines_no`、`uk_mall_refund_lines_item`；
//! - `mall_refund_allocation`：「`(mall_refund_line_id, allocation_no)` 唯一」→
//!   `uk_mall_refund_allocations_no`；「非空 `reverses_allocation_id` 唯一」→
//!   `uk_mall_refund_allocations_reverses`（稀疏唯一）；按原消费取分配 →
//!   `idx_mall_refund_allocations_consumption`；
//! - `mall_balance_restoration`：「`mall_order_fact_id` 非空且唯一」→
//!   `uk_mall_balance_restorations_fact`；「商城 + 恢复单号 + 版本唯一」→
//!   `uk_mall_balance_restorations_identity`；按售后案件回溯 →
//!   `idx_mall_balance_restorations_after_sales_request`；
//! - `mall_balance_restoration_allocation`：「`(mall_balance_restoration_id,
//!   allocation_no)` 唯一」→ `uk_mall_balance_restoration_allocations_no`；
//!   按原 CARD 退款分配取数 → `idx_mall_balance_restoration_allocations_refund`。

use mongodb::{
    bson::{doc, Document},
    options::IndexOptions,
    Database, IndexModel,
};

use crate::repository::extensions::MallAfterSalesExt;
use crate::Result;

/// `mall_after_sales_request` 集合名。
pub(crate) const MALL_AFTER_SALES_REQUESTS: &str =
    <mongodb::Database as MallAfterSalesExt>::MALL_AFTER_SALES_REQUESTS;
/// `mall_after_sales_request_line` 集合名。
pub(crate) const MALL_AFTER_SALES_REQUEST_LINES: &str =
    <mongodb::Database as MallAfterSalesExt>::MALL_AFTER_SALES_REQUEST_LINES;
/// `mall_refund` 集合名。
pub(crate) const MALL_REFUNDS: &str = <mongodb::Database as MallAfterSalesExt>::MALL_REFUNDS;
/// `mall_refund_line` 集合名。
pub(crate) const MALL_REFUND_LINES: &str = <mongodb::Database as MallAfterSalesExt>::MALL_REFUND_LINES;
/// `mall_refund_allocation` 集合名。
pub(crate) const MALL_REFUND_ALLOCATIONS: &str =
    <mongodb::Database as MallAfterSalesExt>::MALL_REFUND_ALLOCATIONS;
/// `mall_balance_restoration` 集合名。
pub(crate) const MALL_BALANCE_RESTORATIONS: &str =
    <mongodb::Database as MallAfterSalesExt>::MALL_BALANCE_RESTORATIONS;
/// `mall_balance_restoration_allocation` 集合名。
pub(crate) const MALL_BALANCE_RESTORATION_ALLOCATIONS: &str =
    <mongodb::Database as MallAfterSalesExt>::MALL_BALANCE_RESTORATION_ALLOCATIONS;

/// 创建本域集合的幂等命名索引。
///
/// 逐条落地数据模型 §6.18「必需约束与索引」；唯一约束一律用唯一索引表达。
/// `REVERSE` 分配引用「非空才唯一」用**稀疏唯一索引**表达：非稀疏唯一索引会
/// 把缺失字段视为 `null` 且只允许一个文档为空，而 `APPLY` 分配恰好没有该字段，
/// 必须 `sparse` 才能表达「非空唯一」。回滚方式：删除稀疏唯一索引，改由
/// 分配序号唯一索引 + P3 等额引用校验兜底。
///
/// # 参数
/// * `db` - 目标 MongoDB 数据库
///
/// # 错误
/// 当已有数据违反唯一约束或 MongoDB 无法创建索引时返回错误。
pub(crate) async fn ensure(db: &Database) -> Result<()> {
    create_indexes(db, MALL_AFTER_SALES_REQUESTS, after_sales_request_indexes()).await?;
    create_indexes(db, MALL_AFTER_SALES_REQUEST_LINES, request_line_indexes()).await?;
    create_indexes(db, MALL_REFUNDS, refund_indexes()).await?;
    create_indexes(db, MALL_REFUND_LINES, refund_line_indexes()).await?;
    create_indexes(db, MALL_REFUND_ALLOCATIONS, refund_allocation_indexes()).await?;
    create_indexes(db, MALL_BALANCE_RESTORATIONS, balance_restoration_indexes()).await?;
    create_indexes(
        db,
        MALL_BALANCE_RESTORATION_ALLOCATIONS,
        balance_restoration_allocation_indexes(),
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

/// 返回 `mall_after_sales_request` 的身份唯一与查询索引。
fn after_sales_request_indexes() -> Vec<IndexModel> {
    vec![
        unique_index(
            "uk_mall_after_sales_requests_identity",
            doc! { "mall_id": 1, "external_request_id": 1 },
        ),
        named_index(
            "idx_mall_after_sales_requests_order_status",
            doc! { "mall_order_id": 1, "status": 1 },
        ),
    ]
}

/// 返回 `mall_after_sales_request_line` 的行号/商品唯一与状态查询索引。
fn request_line_indexes() -> Vec<IndexModel> {
    vec![
        unique_index(
            "uk_mall_after_sales_request_lines_no",
            doc! { "after_sales_request_id": 1, "line_no": 1 },
        ),
        unique_index(
            "uk_mall_after_sales_request_lines_item",
            doc! { "after_sales_request_id": 1, "mall_order_item_id": 1 },
        ),
        named_index(
            "idx_mall_after_sales_request_lines_item_status",
            doc! { "mall_order_item_id": 1, "line_status": 1 },
        ),
    ]
}

/// 返回 `mall_refund` 的事实唯一、退款身份唯一与案件查询索引。
fn refund_indexes() -> Vec<IndexModel> {
    vec![
        unique_index("uk_mall_refunds_fact", doc! { "mall_order_fact_id": 1 }),
        unique_index(
            "uk_mall_refunds_identity",
            doc! { "mall_id": 1, "external_refund_no": 1, "external_refund_version": 1 },
        ),
        named_index(
            "idx_mall_refunds_after_sales_request",
            doc! { "after_sales_request_id": 1 },
        ),
    ]
}

/// 返回 `mall_refund_line` 的行号/商品唯一索引。
fn refund_line_indexes() -> Vec<IndexModel> {
    vec![
        unique_index(
            "uk_mall_refund_lines_no",
            doc! { "mall_refund_id": 1, "line_no": 1 },
        ),
        unique_index(
            "uk_mall_refund_lines_item",
            doc! { "mall_refund_id": 1, "mall_order_item_id": 1 },
        ),
    ]
}

/// 返回 `mall_refund_allocation` 的分配序号唯一、反向引用与消费查询索引。
fn refund_allocation_indexes() -> Vec<IndexModel> {
    vec![
        unique_index(
            "uk_mall_refund_allocations_no",
            doc! { "mall_refund_line_id": 1, "allocation_no": 1 },
        ),
        sparse_unique_index(
            "uk_mall_refund_allocations_reverses",
            doc! { "reverses_allocation_id": 1 },
        ),
        named_index(
            "idx_mall_refund_allocations_consumption",
            doc! { "original_consumption_entry_id": 1 },
        ),
    ]
}

/// 返回 `mall_balance_restoration` 的事实唯一、恢复身份唯一与案件查询索引。
fn balance_restoration_indexes() -> Vec<IndexModel> {
    vec![
        unique_index(
            "uk_mall_balance_restorations_fact",
            doc! { "mall_order_fact_id": 1 },
        ),
        unique_index(
            "uk_mall_balance_restorations_identity",
            doc! { "mall_id": 1, "external_restoration_no": 1, "version": 1 },
        ),
        named_index(
            "idx_mall_balance_restorations_after_sales_request",
            doc! { "after_sales_request_id": 1 },
        ),
    ]
}

/// 返回 `mall_balance_restoration_allocation` 的序号唯一与原退款分配查询索引。
fn balance_restoration_allocation_indexes() -> Vec<IndexModel> {
    vec![
        unique_index(
            "uk_mall_balance_restoration_allocations_no",
            doc! { "mall_balance_restoration_id": 1, "allocation_no": 1 },
        ),
        named_index(
            "idx_mall_balance_restoration_allocations_refund",
            doc! { "mall_refund_allocation_id": 1 },
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

/// 构建命名稀疏唯一索引（只对存在的字段施加唯一约束）。
fn sparse_unique_index(name: impl Into<String>, keys: Document) -> IndexModel {
    IndexModel::builder()
        .keys(keys)
        .options(
            IndexOptions::builder()
                .name(name.into())
                .unique(true)
                .sparse(true)
                .build(),
        )
        .build()
}

#[cfg(test)]
mod tests {
    use mongodb::bson::doc;

    use super::{
        after_sales_request_indexes, balance_restoration_allocation_indexes, balance_restoration_indexes,
        refund_allocation_indexes, refund_indexes, refund_line_indexes, request_line_indexes,
    };

    #[test]
    fn after_sales_request_indexes_cover_identity_and_query() {
        let indexes = after_sales_request_indexes();

        assert!(indexes.iter().any(|index| {
            index.options.as_ref().and_then(|options| options.name.as_deref())
                == Some("uk_mall_after_sales_requests_identity")
                && index.options.as_ref().and_then(|options| options.unique) == Some(true)
                && index.keys == doc! { "mall_id": 1, "external_request_id": 1 }
        }));
        assert!(indexes
            .iter()
            .any(|index| { index.keys == doc! { "mall_order_id": 1, "status": 1 } }));
    }

    #[test]
    fn request_line_indexes_cover_no_and_item_uniqueness() {
        let indexes = request_line_indexes();
        for name in [
            "uk_mall_after_sales_request_lines_no",
            "uk_mall_after_sales_request_lines_item",
        ] {
            assert!(indexes.iter().any(|index| {
                index.options.as_ref().and_then(|options| options.name.as_deref()) == Some(name)
                    && index.options.as_ref().and_then(|options| options.unique) == Some(true)
            }));
        }
        assert!(indexes
            .iter()
            .any(|index| { index.keys == doc! { "mall_order_item_id": 1, "line_status": 1 } }));
    }

    #[test]
    fn refund_indexes_cover_fact_identity_and_after_sales_query() {
        let indexes = refund_indexes();

        assert!(indexes.iter().any(|index| {
            index.options.as_ref().and_then(|options| options.name.as_deref()) == Some("uk_mall_refunds_fact")
                && index.options.as_ref().and_then(|options| options.unique) == Some(true)
                && index.keys == doc! { "mall_order_fact_id": 1 }
        }));
        assert!(indexes.iter().any(|index| {
            index.options.as_ref().and_then(|options| options.name.as_deref())
                == Some("uk_mall_refunds_identity")
                && index.options.as_ref().and_then(|options| options.unique) == Some(true)
                && index.keys
                    == doc! {
                        "mall_id": 1,
                        "external_refund_no": 1,
                        "external_refund_version": 1,
                    }
        }));

        let lines = refund_line_indexes();
        for name in ["uk_mall_refund_lines_no", "uk_mall_refund_lines_item"] {
            assert!(lines.iter().any(|index| {
                index.options.as_ref().and_then(|options| options.name.as_deref()) == Some(name)
                    && index.options.as_ref().and_then(|options| options.unique) == Some(true)
            }));
        }
    }

    #[test]
    fn refund_allocation_uses_sparse_unique_for_reverse_reference() {
        let indexes = refund_allocation_indexes();

        assert!(indexes.iter().any(|index| {
            index.options.as_ref().and_then(|options| options.name.as_deref())
                == Some("uk_mall_refund_allocations_no")
                && index.options.as_ref().and_then(|options| options.unique) == Some(true)
        }));
        let reverses = indexes
            .iter()
            .find(|index| {
                index.options.as_ref().and_then(|options| options.name.as_deref())
                    == Some("uk_mall_refund_allocations_reverses")
            })
            .unwrap();
        assert_eq!(reverses.keys, doc! { "reverses_allocation_id": 1 });
        assert_eq!(reverses.options.as_ref().unwrap().unique, Some(true));
        assert_eq!(reverses.options.as_ref().unwrap().sparse, Some(true));
        assert!(indexes
            .iter()
            .any(|index| { index.keys == doc! { "original_consumption_entry_id": 1 } }));
    }

    #[test]
    fn balance_restoration_indexes_cover_fact_identity_and_allocation_no() {
        let indexes = balance_restoration_indexes();
        assert!(indexes.iter().any(|index| {
            index.options.as_ref().and_then(|options| options.name.as_deref())
                == Some("uk_mall_balance_restorations_fact")
                && index.options.as_ref().and_then(|options| options.unique) == Some(true)
        }));
        assert!(indexes.iter().any(|index| {
            index.options.as_ref().and_then(|options| options.name.as_deref())
                == Some("uk_mall_balance_restorations_identity")
                && index.options.as_ref().and_then(|options| options.unique) == Some(true)
        }));

        let allocations = balance_restoration_allocation_indexes();
        assert!(allocations.iter().any(|index| {
            index.options.as_ref().and_then(|options| options.name.as_deref())
                == Some("uk_mall_balance_restoration_allocations_no")
                && index.options.as_ref().and_then(|options| options.unique) == Some(true)
        }));
        assert!(allocations
            .iter()
            .any(|index| { index.keys == doc! { "mall_refund_allocation_id": 1 } }));
    }
}
