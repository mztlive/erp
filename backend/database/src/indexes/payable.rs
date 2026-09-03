//! 域 D19 `payable` 的索引声明：payable_account、payable_entry、
//! payable_entry_offset、supplier_payment、payment_allocation、
//! purchase_invoice_allocation。
//!
//! 集合名常量取 `PayableExt` 关联常量（唯一权威来源，conventions §4.3）：
//! `indexes/` 与 `repository/` 均为冻结声明下的私有子树，模块路径无法互相引用，
//! 关联常量随 trait 公开可达，两侧共用同一值，禁止字面量重复。

use mongodb::{
    bson::{doc, Document},
    options::IndexOptions,
    Database, IndexModel,
};

use crate::repository::extensions::PayableExt;
use crate::Result;

/// `payable_account` 集合名。
pub(crate) const PAYABLE_ACCOUNTS: &str = <mongodb::Database as PayableExt>::PAYABLE_ACCOUNTS;
/// `payable_entry` 集合名。
pub(crate) const PAYABLE_ENTRIES: &str = <mongodb::Database as PayableExt>::PAYABLE_ENTRIES;
/// `payable_entry_offset` 集合名。
pub(crate) const PAYABLE_ENTRY_OFFSETS: &str = <mongodb::Database as PayableExt>::PAYABLE_ENTRY_OFFSETS;
/// `supplier_payment` 集合名。
pub(crate) const SUPPLIER_PAYMENTS: &str = <mongodb::Database as PayableExt>::SUPPLIER_PAYMENTS;
/// `payment_allocation` 集合名。
pub(crate) const PAYMENT_ALLOCATIONS: &str = <mongodb::Database as PayableExt>::PAYMENT_ALLOCATIONS;
/// `purchase_invoice_allocation` 集合名。
pub(crate) const PURCHASE_INVOICE_ALLOCATIONS: &str =
    <mongodb::Database as PayableExt>::PURCHASE_INVOICE_ALLOCATIONS;

/// 创建本域集合的幂等命名索引。
///
/// 逐条落地数据模型 §6.9「必需约束与索引」。应付子账与付款单的身份类字段使用
/// **全局唯一索引**（与 accounts 的 code 处理一致）：软删除后仍保留身份，避免
/// 复用破坏来源追溯与恢复语义。
///
/// # 参数
/// * `db` - 目标 MongoDB 数据库
///
/// # 错误
/// 当已有数据违反唯一约束或 MongoDB 无法创建索引时返回错误。
pub(crate) async fn ensure(db: &Database) -> Result<()> {
    create_indexes(db, PAYABLE_ACCOUNTS, payable_account_indexes()).await?;
    create_indexes(db, PAYABLE_ENTRIES, payable_entry_indexes()).await?;
    create_indexes(db, PAYABLE_ENTRY_OFFSETS, payable_entry_offset_indexes()).await?;
    create_indexes(db, SUPPLIER_PAYMENTS, supplier_payment_indexes()).await?;
    create_indexes(db, PAYMENT_ALLOCATIONS, payment_allocation_indexes()).await?;
    create_indexes(
        db,
        PURCHASE_INVOICE_ALLOCATIONS,
        purchase_invoice_allocation_indexes(),
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

/// 返回 `payable_account` 的来源唯一与账龄索引。
fn payable_account_indexes() -> Vec<IndexModel> {
    vec![
        unique_index(
            "uk_payable_accounts_source",
            doc! { "source_type": 1, "source_document_id": 1 },
        ),
        named_index(
            "idx_payable_accounts_aging",
            doc! { "supplier_id": 1, "status": 1 },
        ),
    ]
}

/// 返回 `payable_entry` 的业务幂等与账龄索引。
fn payable_entry_indexes() -> Vec<IndexModel> {
    vec![
        unique_index(
            "uk_payable_entries_identity",
            doc! {
                "payable_account_id": 1,
                "source_fact_type": 1,
                "source_document_id": 1,
                "source_revision_id": 1,
                "entry_type": 1,
                "source_sequence": 1,
            },
        ),
        named_index(
            "idx_payable_entries_account_due",
            doc! { "payable_account_id": 1, "due_date": 1 },
        ),
        named_index(
            "idx_payable_entries_account_direction_due",
            doc! { "payable_account_id": 1, "direction": 1, "due_date": 1, "id": 1 },
        ),
        named_index(
            "idx_payable_entries_source",
            doc! { "source_fact_type": 1, "source_document_id": 1 },
        ),
    ]
}

/// 返回 `payable_entry_offset` 的抵销序号与累计冲减索引。
fn payable_entry_offset_indexes() -> Vec<IndexModel> {
    vec![
        unique_index(
            "uk_payable_entry_offsets_decrease",
            doc! { "decrease_entry_id": 1, "offset_sequence": 1 },
        ),
        named_index(
            "idx_payable_entry_offsets_increase",
            doc! { "increase_entry_id": 1 },
        ),
    ]
}

/// 返回 `supplier_payment` 的单号唯一与往来列表索引。
fn supplier_payment_indexes() -> Vec<IndexModel> {
    vec![
        unique_index("uk_supplier_payments_no", doc! { "payment_no": 1 }),
        named_index(
            "idx_supplier_payments_supplier_status",
            doc! { "supplier_id": 1, "status": 1 },
        ),
    ]
}

/// 返回 `payment_allocation` 的分配序号与双向追溯索引。
fn payment_allocation_indexes() -> Vec<IndexModel> {
    vec![
        unique_index(
            "uk_payment_allocations_payment_seq",
            doc! { "supplier_payment_id": 1, "allocation_seq": 1 },
        ),
        named_index(
            "idx_payment_allocations_entry_time",
            doc! { "payable_entry_id": 1, "allocated_at": 1 },
        ),
        named_index(
            "idx_payment_allocations_reverse",
            doc! { "reverses_allocation_id": 1 },
        ),
    ]
}

/// 返回 `purchase_invoice_allocation` 的分配序号与账户追溯索引。
///
/// `idx_purchase_invoice_allocations_account_page` 与
/// `idx_purchase_invoice_allocations_invoice_page` 为 FIN-R06 服务端分页的
/// 复合索引：等值条件前缀 + `(created_at, id)` 稳定排序。同秒记录跨页
/// 无重复、无遗漏。迁移经 `ensure` 幂等创建；回滚按索引名执行
/// `drop_index`（先回滚应用代码再移除索引）。
fn purchase_invoice_allocation_indexes() -> Vec<IndexModel> {
    vec![
        unique_index(
            "uk_purchase_invoice_allocations_invoice_seq",
            doc! { "invoice_id": 1, "allocation_seq": 1 },
        ),
        named_index(
            "idx_purchase_invoice_allocations_account",
            doc! { "payable_account_id": 1 },
        ),
        named_index(
            "idx_purchase_invoice_allocations_account_page",
            doc! { "payable_account_id": 1, "created_at": 1, "id": 1 },
        ),
        named_index(
            "idx_purchase_invoice_allocations_invoice_page",
            doc! { "invoice_id": 1, "created_at": 1, "id": 1 },
        ),
        named_index(
            "idx_purchase_invoice_allocations_reverse",
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
    use mongodb::bson::doc;
    use mongodb::IndexModel;

    use super::{
        payable_account_indexes, payable_entry_indexes, payable_entry_offset_indexes,
        payment_allocation_indexes, purchase_invoice_allocation_indexes, supplier_payment_indexes,
    };

    fn name(index: &IndexModel) -> Option<&str> {
        index.options.as_ref().and_then(|options| options.name.as_deref())
    }

    #[test]
    fn payable_account_source_identity_is_unique() {
        let indexes = payable_account_indexes();

        let identity = indexes
            .iter()
            .find(|index| name(index) == Some("uk_payable_accounts_source"))
            .unwrap();
        assert_eq!(identity.keys, doc! { "source_type": 1, "source_document_id": 1 });
        assert_eq!(identity.options.as_ref().unwrap().unique, Some(true));
        assert!(indexes
            .iter()
            .any(|index| name(index) == Some("idx_payable_accounts_aging")));
    }

    #[test]
    fn payable_entry_identity_covers_the_full_business_key() {
        let indexes = payable_entry_indexes();
        let identity = indexes
            .iter()
            .find(|index| name(index) == Some("uk_payable_entries_identity"))
            .unwrap();
        assert_eq!(
            identity.keys,
            doc! {
                "payable_account_id": 1,
                "source_fact_type": 1,
                "source_document_id": 1,
                "source_revision_id": 1,
                "entry_type": 1,
                "source_sequence": 1,
            }
        );
        assert_eq!(identity.options.as_ref().unwrap().unique, Some(true));
        let direction_due = indexes
            .iter()
            .find(|index| name(index) == Some("idx_payable_entries_account_direction_due"))
            .unwrap();
        assert_eq!(
            direction_due.keys,
            doc! { "payable_account_id": 1, "direction": 1, "due_date": 1, "id": 1 }
        );
        assert_eq!(direction_due.options.as_ref().unwrap().unique, None);
    }

    #[test]
    fn payment_allocations_cover_unique_and_bidirectional_lookups() {
        assert!(payment_allocation_indexes()
            .iter()
            .any(|index| name(index) == Some("uk_payment_allocations_payment_seq")));
        assert!(payment_allocation_indexes().iter().any(|index| {
            name(index) == Some("idx_payment_allocations_entry_time")
                && index.keys == doc! { "payable_entry_id": 1, "allocated_at": 1 }
        }));
        assert!(purchase_invoice_allocation_indexes()
            .iter()
            .any(|index| name(index) == Some("uk_purchase_invoice_allocations_invoice_seq")));
        assert!(purchase_invoice_allocation_indexes()
            .iter()
            .any(|index| index.keys == doc! { "payable_account_id": 1 }));
        assert!(purchase_invoice_allocation_indexes().iter().any(|index| {
            name(index) == Some("idx_purchase_invoice_allocations_account_page")
                && index.keys == doc! { "payable_account_id": 1, "created_at": 1, "id": 1 }
        }));
        assert!(purchase_invoice_allocation_indexes().iter().any(|index| {
            name(index) == Some("idx_purchase_invoice_allocations_invoice_page")
                && index.keys == doc! { "invoice_id": 1, "created_at": 1, "id": 1 }
        }));
        assert!(supplier_payment_indexes()
            .iter()
            .any(|index| name(index) == Some("uk_supplier_payments_no")));
        assert!(payable_entry_offset_indexes()
            .iter()
            .any(|index| index.keys == doc! { "increase_entry_id": 1 }));
    }
}
