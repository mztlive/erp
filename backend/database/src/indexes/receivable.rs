//! 域 D18 `receivable` 的索引声明：receivable_account、receivable_entry、
//! receivable_funds_review、receivable_entry_offset、customer_receipt、
//! receipt_allocation、invoice、sales_invoice_allocation。
//!
//! 集合名常量取 `ReceivableExt` 关联常量（唯一权威来源，conventions §4.3）：
//! `indexes/` 与 `repository/` 均为冻结声明下的私有子树，模块路径无法互相引用，
//! 关联常量随 trait 公开可达，两侧共用同一值，禁止字面量重复。

use mongodb::{
    bson::{doc, Document},
    options::IndexOptions,
    Database, IndexModel,
};

use crate::repository::extensions::ReceivableExt;
use crate::Result;

/// `receivable_account` 集合名。
pub(crate) const RECEIVABLE_ACCOUNTS: &str = <mongodb::Database as ReceivableExt>::RECEIVABLE_ACCOUNTS;
/// `receivable_entry` 集合名。
pub(crate) const RECEIVABLE_ENTRIES: &str = <mongodb::Database as ReceivableExt>::RECEIVABLE_ENTRIES;
/// `receivable_funds_review` 集合名。
pub(crate) const RECEIVABLE_FUNDS_REVIEWS: &str =
    <mongodb::Database as ReceivableExt>::RECEIVABLE_FUNDS_REVIEWS;
/// `receivable_entry_offset` 集合名。
pub(crate) const RECEIVABLE_ENTRY_OFFSETS: &str =
    <mongodb::Database as ReceivableExt>::RECEIVABLE_ENTRY_OFFSETS;
/// `customer_receipt` 集合名。
pub(crate) const CUSTOMER_RECEIPTS: &str = <mongodb::Database as ReceivableExt>::CUSTOMER_RECEIPTS;
/// `receipt_allocation` 集合名。
pub(crate) const RECEIPT_ALLOCATIONS: &str = <mongodb::Database as ReceivableExt>::RECEIPT_ALLOCATIONS;
/// `invoice` 集合名。
pub(crate) const INVOICES: &str = <mongodb::Database as ReceivableExt>::INVOICES;
/// `sales_invoice_allocation` 集合名。
pub(crate) const SALES_INVOICE_ALLOCATIONS: &str =
    <mongodb::Database as ReceivableExt>::SALES_INVOICE_ALLOCATIONS;

/// 创建本域集合的幂等命名索引。
///
/// 逐条落地数据模型 §6.8「必需约束与索引」。账户身份类字段使用**全局唯一索引**
/// （与 accounts 的 code 处理一致）：软删除后仍保留身份，避免复用破坏来源追溯
/// 与恢复语义。无代码数电票的「(invoice_direction, normalized_no) 唯一」用
/// **部分唯一索引**表达：`normalized_code` 为空的文档才参与唯一判定，有代码
/// 发票互不干扰（理由与回滚见 [`uncoded_index_options`] 注释）。
///
/// # 参数
/// * `db` - 目标 MongoDB 数据库
///
/// # 错误
/// 当已有数据违反唯一约束或 MongoDB 无法创建索引时返回错误。
pub(crate) async fn ensure(db: &Database) -> Result<()> {
    create_indexes(db, RECEIVABLE_ACCOUNTS, receivable_account_indexes()).await?;
    create_indexes(db, RECEIVABLE_ENTRIES, receivable_entry_indexes()).await?;
    create_indexes(db, RECEIVABLE_FUNDS_REVIEWS, receivable_funds_review_indexes()).await?;
    create_indexes(db, RECEIVABLE_ENTRY_OFFSETS, receivable_entry_offset_indexes()).await?;
    create_indexes(db, CUSTOMER_RECEIPTS, customer_receipt_indexes()).await?;
    create_indexes(db, RECEIPT_ALLOCATIONS, receipt_allocation_indexes()).await?;
    create_indexes(db, INVOICES, invoice_indexes()).await?;
    create_indexes(db, SALES_INVOICE_ALLOCATIONS, sales_invoice_allocation_indexes()).await?;
    Ok(())
}

/// 为单个集合创建一组幂等命名索引。
async fn create_indexes(db: &Database, collection: &str, indexes: Vec<IndexModel>) -> Result<()> {
    db.collection::<Document>(collection)
        .create_indexes(indexes)
        .await?;
    Ok(())
}

/// 返回 `receivable_account` 的身份约束与往来列表索引。
fn receivable_account_indexes() -> Vec<IndexModel> {
    vec![
        unique_index(
            "uk_receivable_accounts_sales_order",
            doc! { "sales_order_id": 1, "account_seq": 1 },
        ),
        named_index(
            "idx_receivable_accounts_aging",
            doc! { "counterparty_party_id": 1, "status": 1 },
        ),
        named_index(
            "idx_receivable_accounts_customer",
            doc! { "customer_id": 1, "status": 1 },
        ),
    ]
}

/// 返回 `receivable_entry` 的业务幂等与账龄索引。
fn receivable_entry_indexes() -> Vec<IndexModel> {
    vec![
        unique_index(
            "uk_receivable_entries_identity",
            doc! {
                "receivable_account_id": 1,
                "source_fact_type": 1,
                "source_document_id": 1,
                "source_revision_id": 1,
                "entry_type": 1,
                "source_sequence": 1,
            },
        ),
        named_index(
            "idx_receivable_entries_account_due",
            doc! { "receivable_account_id": 1, "due_date": 1 },
        ),
        named_index(
            "idx_receivable_entries_source",
            doc! { "source_fact_type": 1, "source_document_id": 1 },
        ),
    ]
}

/// 返回 `receivable_funds_review` 的复核链约束索引。
fn receivable_funds_review_indexes() -> Vec<IndexModel> {
    vec![
        unique_index(
            "uk_receivable_funds_reviews_account_no",
            doc! { "receivable_account_id": 1, "review_no": 1 },
        ),
        unique_index(
            "uk_receivable_funds_reviews_work_item",
            doc! { "work_item_id": 1 },
        ),
        // §6.8：非空 supersedes_review_id 唯一且必须属于同一子账。MongoDB 唯一
        // 索引对缺失/null 值不去重，等价于「非空值唯一」的部分唯一语义，无需
        // partialFilterExpression；回滚：改为应用层校验后删除此索引。
        unique_index(
            "uk_receivable_funds_reviews_supersedes",
            doc! { "supersedes_review_id": 1 },
        ),
    ]
}

/// 返回 `receivable_entry_offset` 的抵销序号与累计冲减索引。
fn receivable_entry_offset_indexes() -> Vec<IndexModel> {
    vec![
        unique_index(
            "uk_receivable_entry_offsets_decrease",
            doc! { "decrease_entry_id": 1, "offset_sequence": 1 },
        ),
        named_index(
            "idx_receivable_entry_offsets_increase",
            doc! { "increase_entry_id": 1 },
        ),
    ]
}

/// 返回 `customer_receipt` 的单号唯一与往来列表索引。
fn customer_receipt_indexes() -> Vec<IndexModel> {
    vec![
        unique_index("uk_customer_receipts_no", doc! { "receipt_no": 1 }),
        named_index(
            "idx_customer_receipts_party_status",
            doc! { "counterparty_party_id": 1, "status": 1 },
        ),
    ]
}

/// 返回 `receipt_allocation` 的分配序号与反向追溯索引。
fn receipt_allocation_indexes() -> Vec<IndexModel> {
    vec![
        unique_index(
            "uk_receipt_allocations_receipt_seq",
            doc! { "customer_receipt_id": 1, "allocation_seq": 1 },
        ),
        named_index(
            "idx_receipt_allocations_entry_time",
            doc! { "receivable_entry_id": 1, "allocated_at": 1 },
        ),
        named_index(
            "idx_receipt_allocations_reverse",
            doc! { "reverses_allocation_id": 1 },
        ),
    ]
}

/// 构建无代码数电票唯一索引的选项（部分唯一，`normalized_code` 为空才参与）。
///
/// §6.8：有代码发票按 `(invoice_direction, normalized_code, normalized_no)` 唯一，
/// 无代码数电票按 `(invoice_direction, normalized_no)` 唯一。MongoDB 唯一索引对
/// null 值不去重，因此有代码唯一索引天然放过无代码发票；「无代码」唯一约束必须
/// 用部分唯一索引限定 `normalized_code: null` 的文档，否则两张不同代码、同号码
/// 的有代码发票会被误判重复。回滚：改为应用层登记前去重后删除本索引。
fn uncoded_index_options() -> IndexOptions {
    IndexOptions::builder()
        .name("uk_invoices_uncoded".to_string())
        .unique(true)
        .partial_filter_expression(doc! { "normalized_code": null })
        .build()
}

/// 返回 `invoice` 的登记唯一与查询索引。
fn invoice_indexes() -> Vec<IndexModel> {
    vec![
        unique_index(
            "uk_invoices_coded",
            doc! { "invoice_direction": 1, "normalized_code": 1, "normalized_no": 1 },
        ),
        IndexModel::builder()
            .keys(doc! { "invoice_direction": 1, "normalized_no": 1 })
            .options(uncoded_index_options())
            .build(),
        named_index("idx_invoices_party_status", doc! { "party_id": 1, "status": 1 }),
        named_index("idx_invoices_original", doc! { "original_invoice_id": 1 }),
    ]
}

/// 返回 `sales_invoice_allocation` 的分配序号与账户追溯索引。
fn sales_invoice_allocation_indexes() -> Vec<IndexModel> {
    vec![
        unique_index(
            "uk_sales_invoice_allocations_invoice_seq",
            doc! { "invoice_id": 1, "allocation_seq": 1 },
        ),
        named_index(
            "idx_sales_invoice_allocations_account",
            doc! { "receivable_account_id": 1 },
        ),
        named_index(
            "idx_sales_invoice_allocations_reverse",
            doc! { "reverses_allocation_id": 1 },
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
    use mongodb::bson::{doc, Bson};
    use mongodb::IndexModel;

    use super::{
        customer_receipt_indexes, invoice_indexes, receipt_allocation_indexes, receivable_account_indexes,
        receivable_entry_indexes, receivable_entry_offset_indexes, receivable_funds_review_indexes,
        sales_invoice_allocation_indexes, uncoded_index_options,
    };

    fn name(index: &IndexModel) -> Option<&str> {
        index.options.as_ref().and_then(|options| options.name.as_deref())
    }

    #[test]
    fn receivable_account_identity_is_unique_and_aging_indexes_exist() {
        let indexes = receivable_account_indexes();

        let identity = indexes
            .iter()
            .find(|index| name(index) == Some("uk_receivable_accounts_sales_order"))
            .unwrap();
        assert_eq!(identity.keys, doc! { "sales_order_id": 1, "account_seq": 1 });
        assert_eq!(identity.options.as_ref().unwrap().unique, Some(true));

        assert!(indexes
            .iter()
            .any(|index| name(index) == Some("idx_receivable_accounts_aging")));
        assert!(indexes
            .iter()
            .any(|index| name(index) == Some("idx_receivable_accounts_customer")));
    }

    #[test]
    fn receivable_entry_identity_covers_the_full_business_key() {
        let indexes = receivable_entry_indexes();

        let identity = indexes
            .iter()
            .find(|index| name(index) == Some("uk_receivable_entries_identity"))
            .unwrap();
        assert_eq!(
            identity.keys,
            doc! {
                "receivable_account_id": 1,
                "source_fact_type": 1,
                "source_document_id": 1,
                "source_revision_id": 1,
                "entry_type": 1,
                "source_sequence": 1,
            }
        );
        assert_eq!(identity.options.as_ref().unwrap().unique, Some(true));
    }

    #[test]
    fn funds_review_constraints_are_unique() {
        let indexes = receivable_funds_review_indexes();

        assert!(indexes.iter().any(|index| {
            name(index) == Some("uk_receivable_funds_reviews_account_no")
                && index.options.as_ref().and_then(|o| o.unique) == Some(true)
        }));
        assert!(indexes.iter().any(|index| {
            name(index) == Some("uk_receivable_funds_reviews_work_item")
                && index.options.as_ref().and_then(|o| o.unique) == Some(true)
        }));
        assert!(indexes.iter().any(|index| {
            name(index) == Some("uk_receivable_funds_reviews_supersedes")
                && index.keys == doc! { "supersedes_review_id": 1 }
        }));
    }

    #[test]
    fn receipt_and_offset_indexes_cover_unique_and_reverse_lookups() {
        assert!(receipt_allocation_indexes()
            .iter()
            .any(|index| name(index) == Some("uk_receipt_allocations_receipt_seq")));
        assert!(receipt_allocation_indexes().iter().any(|index| {
            name(index) == Some("idx_receipt_allocations_entry_time")
                && index.keys == doc! { "receivable_entry_id": 1, "allocated_at": 1 }
        }));
        assert!(receivable_entry_offset_indexes()
            .iter()
            .any(|index| name(index) == Some("uk_receivable_entry_offsets_decrease")));
        assert!(receivable_entry_offset_indexes()
            .iter()
            .any(|index| index.keys == doc! { "increase_entry_id": 1 }));
        assert!(customer_receipt_indexes()
            .iter()
            .any(|index| name(index) == Some("uk_customer_receipts_no")));
    }

    #[test]
    fn invoice_coded_and_partial_uncoded_uniques_coexist() {
        let indexes = invoice_indexes();

        let coded = indexes
            .iter()
            .find(|index| name(index) == Some("uk_invoices_coded"))
            .unwrap();
        assert_eq!(
            coded.keys,
            doc! { "invoice_direction": 1, "normalized_code": 1, "normalized_no": 1 }
        );
        assert_eq!(coded.options.as_ref().unwrap().unique, Some(true));

        let uncoded_options = uncoded_index_options();
        assert_eq!(uncoded_options.name.as_deref(), Some("uk_invoices_uncoded"));
        assert_eq!(uncoded_options.unique, Some(true));
        assert_eq!(
            uncoded_options.partial_filter_expression,
            Some(doc! { "normalized_code": null })
        );
        let uncoded = indexes
            .iter()
            .find(|index| name(index) == Some("uk_invoices_uncoded"))
            .unwrap();
        assert_eq!(uncoded.keys, doc! { "invoice_direction": 1, "normalized_no": 1 });
    }

    #[test]
    fn sales_invoice_allocation_seq_is_unique_per_invoice() {
        let indexes = sales_invoice_allocation_indexes();

        assert!(indexes.iter().any(|index| {
            name(index) == Some("uk_sales_invoice_allocations_invoice_seq")
                && index.keys == doc! { "invoice_id": 1, "allocation_seq": 1 }
                && index.options.as_ref().and_then(|o| o.unique) == Some(true)
        }));
        assert!(indexes
            .iter()
            .any(|index| index.keys == doc! { "receivable_account_id": 1 }));
        assert!(matches!(
            indexes
                .iter()
                .find(|index| name(index) == Some("idx_sales_invoice_allocations_reverse"))
                .and_then(|index| index.keys.get("reverses_allocation_id")),
            Some(Bson::Int32(1))
        ));
    }
}
