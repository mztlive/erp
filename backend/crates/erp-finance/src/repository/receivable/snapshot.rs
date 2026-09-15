//! 应收票款仓储数据面（FIN-R09 / FIN-R12）。
//!
//! 提供有界 `ReceivableSnapshotFacts` 读取，以及回款核销的批量条件进度
//! 更新与 `insert_many`。所有方法接收
//! 调用方 `&mut dyn Executor`，不开启事务，不返回 services DTO。

use std::collections::HashSet;

use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use erp_core::ids::{ReceivableAccountId, ReceivableEntryId};
use erp_core::money::Amount;
use mongodb::bson::{Bson, Document, doc};
use persistence_core::{Executor, Result, mongo_ops};

use super::account::{amount_bson, progress_pipeline};
use super::{ReceivableRepository, SettlementBatchResult};
use crate::entity::receivable::{
    CustomerReceipt, Invoice, ReceiptAllocation, ReceivableEntry, SalesInvoiceAllocation,
};
use crate::repository::extensions::ReceivableExt;
use crate::repository::owned::{ReceivableAccountRepository, ReceivableEntryRepository};

/// 应收票款快照的有界持久化事实（FIN-R12）。
///
/// 固定次数读取：分录、回款分配、发票分配、回款、发票。
/// 缺失回款／发票由 Service 按 `expected_*_count` 解释首错；本类型不裁决业务。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReceivableSnapshotFacts {
    /// 账户分录（按 `source_sequence` 升序）。
    pub entries: Vec<ReceivableEntry>,
    /// 分录关联的回款核销分配。
    pub receipt_allocations: Vec<ReceiptAllocation>,
    /// 账户关联的销项发票分配。
    pub invoice_allocations: Vec<SalesInvoiceAllocation>,
    /// 分配引用的回款单（去重后批量读取）。
    pub receipts: Vec<CustomerReceipt>,
    /// 分配引用的发票（去重后批量读取）。
    pub invoices: Vec<Invoice>,
    /// 去重后应存在的回款单数量。
    pub expected_receipt_count: usize,
    /// 去重后应存在的发票数量。
    pub expected_invoice_count: usize,
}

impl<'a> ReceivableRepository<'a> {
    /// 一次装载应收账户范围内的有界票款事实。
    ///
    /// 读取次数与分录／分配行数无关：分录按账户一次、分配按 ID 集合各一次、
    /// 回款／发票按去重 ID `$in` 各一次。
    ///
    /// # 参数
    /// * `account_id` - 应收往来子账
    /// * `executor` - 调用方执行器，必须复用以保持同一事务快照
    ///
    /// # 返回
    /// 返回 [`ReceivableSnapshotFacts`]；关联单据缺失时仍返回已读到的子集，
    /// 由 Service 按回款优先于发票的首错顺序解释。
    ///
    /// # 错误
    /// MongoDB 查询失败时返回错误。
    ///
    /// # 约束
    /// 不装载销售单（跨聚合），不裁决业务，不开事务。
    pub async fn receivable_snapshot_facts(
        &self,
        account_id: &ReceivableAccountId,
        executor: &mut dyn Executor,
    ) -> Result<ReceivableSnapshotFacts> {
        let entries = self.db.receivable_entries().find_entries_by_account(account_id, executor).await?;
        let entry_ids =
            entries.iter().map(|entry| ReceivableEntryId::new(entry.base.id.clone())).collect::<Vec<_>>();
        let receipt_allocations =
            self.db.receipt_allocations().find_allocations_by_entries(&entry_ids, executor).await?;
        let invoice_allocations = self
            .db
            .sales_invoice_allocations()
            .find_allocations_by_accounts(std::slice::from_ref(account_id), executor)
            .await?;
        let receipt_ids = unique_ids(
            receipt_allocations.iter().map(|allocation| allocation.customer_receipt_id.to_string()),
        );
        let expected_receipt_count = receipt_ids.len();
        let receipts = self.db.customer_receipts().find_receipts_by_ids(&receipt_ids, executor).await?;
        let invoice_ids =
            unique_ids(invoice_allocations.iter().map(|allocation| allocation.invoice_id.to_string()));
        let expected_invoice_count = invoice_ids.len();
        let invoices = self.db.invoices().find_invoices_by_ids(&invoice_ids, executor).await?;
        Ok(ReceivableSnapshotFacts {
            entries,
            receipt_allocations,
            invoice_allocations,
            receipts,
            invoices,
            expected_receipt_count,
            expected_invoice_count,
        })
    }

    /// 批量创建回款核销分配（`insert_many`，调用方事务内原子写入，FIN-R09）。
    ///
    /// # 参数
    /// * `allocations` - 待持久化的回款核销分配
    /// * `executor` - 必须位于事务中
    ///
    /// # 返回
    /// 全部写入成功或空输入时返回 `Ok(())`。
    ///
    /// # 错误
    /// 唯一键冲突透出 [`persistence_core::Error::DuplicateKey`]，由 Service 转译。
    ///
    /// # 约束
    /// 不开启事务；空输入不访问数据库。
    pub async fn create_receipt_allocations_many(
        &self,
        allocations: &[ReceiptAllocation],
        executor: &mut dyn Executor,
    ) -> Result<()> {
        mongo_ops::insert_many(
            &self
                .db
                .collection::<ReceiptAllocation>(<mongodb::Database as ReceivableExt>::RECEIPT_ALLOCATIONS),
            allocations.to_vec(),
            executor,
        )
        .await
    }
}

impl<'a> ReceivableAccountRepository<'a> {
    /// 批量条件核销：按账户聚合增量逐个执行不超额核销（FIN-R09）。
    ///
    /// # 参数
    /// * `deltas` - 按账户聚合的本次核销增量（同一账户只出现一次）
    /// * `updated_by` - 本次更新执行人
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 返回逐账户命中结果；空输入不访问数据库。
    ///
    /// # 错误
    /// MongoDB 更新失败时返回错误。
    ///
    /// # 约束
    /// 聚合口径由领域账本保证；本方法不裁决跨账户业务结论、不开事务。
    pub async fn apply_settlements_many(
        &self,
        deltas: &[(ReceivableAccountId, Amount)],
        updated_by: &str,
        executor: &mut dyn Executor,
    ) -> Result<SettlementBatchResult> {
        let mut applied = Vec::new();
        let mut rejected = Vec::new();
        for (id, amount) in deltas {
            let amount = amount_bson(amount)?;
            let hit = self
                .conditional_update(
                    settlement_guard(id.as_ref(), &amount),
                    progress_pipeline("settled_total", "open_total", &amount, true, updated_by),
                    executor,
                )
                .await?;
            if hit {
                applied.push(id.clone());
            } else {
                rejected.push(id.clone());
            }
        }
        Ok(SettlementBatchResult { applied, rejected })
    }
}

impl<'a> ReceivableEntryRepository<'a> {
    /// 按主键集合批量取回应收分录（`$in` 一次取回，禁止 N+1）。
    ///
    /// # 参数
    /// * `entry_ids` - 应收分录 ID 集合
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 返回全部匹配分录；空集合直接返回空列表。
    ///
    /// # 错误
    /// MongoDB 查询失败时返回错误。
    ///
    /// # 约束
    /// 仓储内不去重业务结论；调用方负责解释缺项。
    pub async fn find_entries_by_ids(
        &self,
        entry_ids: &[ReceivableEntryId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<ReceivableEntry>> {
        if entry_ids.is_empty() {
            return Ok(Vec::new());
        }
        let ids = unique_ids(entry_ids.iter().map(ToString::to_string));
        self.find_many(doc! { "id": { "$in": ids } }, executor).await
    }
}

/// 构造条件核销的写前置条件（不超额核销）。
///
/// # 参数
/// * `id` - 应收往来子账 ID
/// * `amount` - 本次核销含税金额（Decimal128）
///
/// # 返回
/// 返回未删除账户的核销额度守卫文档。
///
/// # 错误
/// 不返回错误。
///
/// # 约束
/// 与单笔 [`Repository::apply_settlement`] 守卫一致。
fn settlement_guard(id: &str, amount: &Bson) -> Document {
    doc! {
        "id": id,
        "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
        "$expr": {
            "$lte": [
                { "$add": ["$settled_total", amount] },
                "$gross_total",
            ],
        },
    }
}

/// 按首次出现顺序去重 ID。
///
/// # 参数
/// * `ids` - ID 迭代
///
/// # 返回
/// 返回去重后的 ID 列表。
///
/// # 错误
/// 不返回错误。
///
/// # 约束
/// 空输入返回空向量。
fn unique_ids(ids: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut unique = Vec::new();
    for id in ids {
        if seen.insert(id.clone()) {
            unique.push(id);
        }
    }
    unique
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use erp_core::ids::{ReceivableAccountId, ReceivableEntryId};
    use erp_core::money::Amount;
    use mongodb::bson::Bson;
    use persistence_core::NoTransaction;

    use super::{ReceivableSnapshotFacts, settlement_guard, unique_ids};
    use crate::entity::receivable::{ReceivableAccount, ReceivableEntry};
    use crate::repository::extensions::ReceivableExt;
    use crate::repository::owned::{ReceivableAccountRepository, ReceivableEntryRepository};
    use crate::repository::receivable::SettlementBatchResult;

    #[test]
    fn settlement_guard_matches_single_apply_filter() {
        let amount = super::super::account::amount_bson(&Amount::from_str("100.50").unwrap()).unwrap();
        let guard = settlement_guard("acct-1", &amount);
        assert_eq!(guard.get_str("id").unwrap(), "acct-1");
        assert_eq!(guard.get_i64("deleted_at").unwrap(), 0);
        let expr = guard.get_document("$expr").unwrap();
        let lte = expr.get_array("$lte").unwrap();
        let add = lte[0].as_document().unwrap().get_array("$add").unwrap();
        assert_eq!(add[0], Bson::String("$settled_total".to_string()));
        assert!(matches!(add[1], Bson::Decimal128(_)));
        assert_eq!(lte[1], Bson::String("$gross_total".to_string()));
    }

    #[test]
    fn unique_ids_preserve_first_occurrence() {
        assert_eq!(
            unique_ids(["b".to_string(), "a".to_string(), "b".to_string()]),
            vec!["b".to_string(), "a".to_string()]
        );
        assert!(unique_ids(Vec::<String>::new()).is_empty());
    }

    #[test]
    fn find_entries_by_ids_uniques_before_query() {
        let source = include_str!("snapshot.rs");
        assert!(source.contains("pub async fn find_entries_by_ids"));
        assert!(source.contains("unique_ids(entry_ids.iter().map(ToString::to_string))"));
    }

    fn test_entry(id: &str, sequence: u32) -> ReceivableEntry {
        use erp_core::common::time::{BusinessDate, Instant};

        use crate::entity::receivable::{
            EntryDirection, ReceivableEntry, ReceivableEntryData, ReceivableEntryType,
        };
        ReceivableEntry::new(
            ReceivableEntryId::new(id),
            ReceivableEntryData {
                receivable_account_id: ReceivableAccountId::new("ra-batch"),
                entry_type: ReceivableEntryType::Original,
                direction: EntryDirection::Increase,
                amount: Amount::from_str("10.00").unwrap(),
                due_date: BusinessDate::from_ymd(2026, 9, 30).unwrap(),
                source_fact_type: "sales_order".to_string(),
                source_document_id: format!("so-{id}"),
                source_revision_id: "sor-1".to_string(),
                source_sequence: sequence,
                posted_at: Instant::from_unix_secs(1_700_000_000),
            },
        )
        .unwrap()
    }

    /// 批量按 ID 读取：空输入、去重、乱序结果、缺项与软删排除。
    #[tokio::test]
    #[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
    async fn find_entries_by_ids_dedups_unordered_missing_and_excludes_soft_deleted() {
        use std::collections::HashSet;

        use crate::repository::test_fixture::{TestDb, require_mongo};

        require_mongo!(async {
            let fixture = TestDb::new("recv_entries_by_ids").await.expect("测试数据库创建失败");
            crate::indexes::ensure(fixture.db()).await.expect("索引创建失败");
            let entries = fixture.db().receivable_entries();
            assert!(
                entries
                    .find_entries_by_ids(&[], &mut NoTransaction)
                    .await
                    .expect("空输入必须成功")
                    .is_empty()
            );

            entries.create(&test_entry("e-1", 1), &mut NoTransaction).await.expect("分录写入失败");
            entries.create(&test_entry("e-2", 2), &mut NoTransaction).await.expect("分录写入失败");
            let mut deleted = test_entry("e-del", 3);
            deleted.base.deleted_at = 1;
            entries.create(&deleted, &mut NoTransaction).await.expect("软删分录写入失败");

            let duplicated = [
                ReceivableEntryId::new("e-2"),
                ReceivableEntryId::new("e-1"),
                ReceivableEntryId::new("e-2"),
                ReceivableEntryId::new("missing"),
                ReceivableEntryId::new("e-del"),
            ];
            let rows =
                entries.find_entries_by_ids(&duplicated, &mut NoTransaction).await.expect("批量读取必须成功");
            let ids = rows.iter().map(|row| row.base.id.clone()).collect::<HashSet<_>>();
            assert_eq!(ids.len(), 2, "重复 ID 不得放大结果");
            assert!(ids.contains("e-1"));
            assert!(ids.contains("e-2"));
            assert!(!ids.contains("missing"), "缺项由调用方解释，仓储不得伪造");
            assert!(!ids.contains("e-del"), "软删分录必须排除");
        });
    }

    #[tokio::test]
    async fn apply_settlements_many_empty_input_returns_empty_without_db() {
        let client =
            mongodb::Client::with_uri_str("mongodb://127.0.0.1:1").await.expect("客户端句柄创建失败");
        let database = client.database("unused");
        let repository: ReceivableAccountRepository<'_> = ReceivableAccountRepository::new(
            &database,
            <mongodb::Database as ReceivableExt>::RECEIVABLE_ACCOUNTS,
        );
        let result = repository
            .apply_settlements_many(&[], "tester", &mut NoTransaction)
            .await
            .expect("空输入批量核销必须成功");
        assert_eq!(result, SettlementBatchResult { applied: Vec::new(), rejected: Vec::new() });
        let _ = ReceivableAccountId::new("acct-1");
    }

    #[tokio::test]
    async fn snapshot_facts_type_is_storage_agnostic() {
        let facts = ReceivableSnapshotFacts {
            entries: Vec::new(),
            receipt_allocations: Vec::new(),
            invoice_allocations: Vec::new(),
            receipts: Vec::new(),
            invoices: Vec::new(),
            expected_receipt_count: 0,
            expected_invoice_count: 0,
        };
        assert_eq!(facts.expected_receipt_count, 0);
        assert_eq!(facts.expected_invoice_count, 0);
    }

    #[tokio::test]
    async fn find_entries_by_ids_empty_input_skips_db() {
        let client =
            mongodb::Client::with_uri_str("mongodb://127.0.0.1:1").await.expect("客户端句柄创建失败");
        let database = client.database("unused");
        let repository: ReceivableEntryRepository<'_> = ReceivableEntryRepository::new(
            &database,
            <mongodb::Database as ReceivableExt>::RECEIVABLE_ENTRIES,
        );
        let rows = repository.find_entries_by_ids(&[], &mut NoTransaction).await.expect("空输入必须成功");
        assert!(rows.is_empty());
    }

    #[tokio::test]
    async fn create_receipt_allocations_many_empty_input_returns_without_db() {
        let client =
            mongodb::Client::with_uri_str("mongodb://127.0.0.1:1").await.expect("客户端句柄创建失败");
        let database = client.database("unused");
        crate::repository::receivable::ReceivableRepository::new(&database)
            .create_receipt_allocations_many(&[], &mut NoTransaction)
            .await
            .expect("空输入批量插入必须成功且不访问数据库");
    }

    fn test_account(id: &str, gross: &str, settled: &str) -> ReceivableAccount {
        use erp_core::ids::{CustomerAccountId, PartyId, SalesOrderId, SalesOrderRevisionId};

        use crate::entity::receivable::{AccountReviewStatus, ReceivableAccountData};
        ReceivableAccount::new(
            ReceivableAccountId::new(id),
            ReceivableAccountData {
                sales_order_id: SalesOrderId::new("so-1"),
                account_seq: 1,
                customer_id: CustomerAccountId::new("cust-1"),
                counterparty_party_id: PartyId::new("party-1"),
                source_sales_order_revision_id: SalesOrderRevisionId::new("sor-1"),
                review_status: AccountReviewStatus::NotApplicable,
                reviewed_by: None,
                reviewed_at: None,
                review_evidence_reference: None,
                gross_total: Amount::from_str(gross).unwrap(),
                settled_total: Amount::from_str(settled).unwrap(),
                invoiceable_total: Amount::from_str(gross).unwrap(),
                invoiced_total: Amount::from_str("0.00").unwrap(),
            },
            "tester",
        )
        .unwrap()
    }

    /// 批量条件核销：聚合增量逐账户生效，超额账户被拒绝且金额不变。
    #[tokio::test]
    #[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
    async fn batch_settlement_applies_aggregated_deltas_and_reports_rejected() {
        use crate::repository::test_fixture::{TestDb, require_mongo};

        require_mongo!(async {
            let fixture = TestDb::new("recv_settle_batch").await.expect("测试数据库创建失败");
            crate::indexes::ensure(fixture.db()).await.expect("索引创建失败");
            let accounts = fixture.db().receivable_accounts();
            accounts
                .create(&test_account("acct-1", "1000.00", "0.00"), &mut NoTransaction)
                .await
                .expect("子账写入失败");
            let mut second = test_account("acct-2", "1000.00", "0.00");
            second.account_seq = 2;
            accounts.create(&second, &mut NoTransaction).await.expect("子账写入失败");

            let deltas = [
                (ReceivableAccountId::new("acct-1"), Amount::from_str("400.00").unwrap()),
                (ReceivableAccountId::new("acct-2"), Amount::from_str("600.00").unwrap()),
            ];
            let result = accounts
                .apply_settlements_many(&deltas, "tester", &mut NoTransaction)
                .await
                .expect("批量核销失败");
            assert!(result.rejected.is_empty());
            let one = accounts
                .find_by_id("acct-1", &mut NoTransaction)
                .await
                .expect("读取失败")
                .expect("子账必须存在");
            assert_eq!(one.settled_total, Amount::from_str("400.00").unwrap());
            assert_eq!(one.open_total, Amount::from_str("600.00").unwrap());

            let over = [(ReceivableAccountId::new("acct-1"), Amount::from_str("700.00").unwrap())];
            let result = accounts
                .apply_settlements_many(&over, "tester", &mut NoTransaction)
                .await
                .expect("超额核销必须报告拒绝");
            assert!(result.applied.is_empty());
            assert_eq!(result.rejected, vec![ReceivableAccountId::new("acct-1")]);
            let one = accounts
                .find_by_id("acct-1", &mut NoTransaction)
                .await
                .expect("读取失败")
                .expect("子账必须存在");
            assert_eq!(one.settled_total, Amount::from_str("400.00").unwrap());
        });
    }

    /// 快照有界读取：缺失回款 count 先于缺失发票 count，供 Service 保持首错顺序。
    #[tokio::test]
    #[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
    async fn snapshot_facts_reports_missing_receipts_before_invoices() {
        use erp_core::common::time::{BusinessDate, Instant};
        use erp_core::ids::{
            CustomerReceiptId, InvoiceId, ReceiptAllocationId, ReceivableEntryId, SalesInvoiceAllocationId,
        };

        use crate::entity::receivable::{
            AllocationAction, EntryDirection, ReceiptAllocation, ReceiptAllocationData, ReceivableEntry,
            ReceivableEntryData, ReceivableEntryType, SalesInvoiceAllocation, SalesInvoiceAllocationData,
        };
        use crate::repository::test_fixture::{TestDb, require_mongo};

        require_mongo!(async {
            let fixture = TestDb::new("recv_snapshot_missing").await.expect("测试数据库创建失败");
            crate::indexes::ensure(fixture.db()).await.expect("索引创建失败");
            let db = fixture.db();
            db.receivable_accounts()
                .create(&test_account("ra-1", "100.00", "0.00"), &mut NoTransaction)
                .await
                .expect("子账写入失败");
            db.receivable_entries()
                .create(
                    &ReceivableEntry::new(
                        ReceivableEntryId::new("e-1"),
                        ReceivableEntryData {
                            receivable_account_id: ReceivableAccountId::new("ra-1"),
                            entry_type: ReceivableEntryType::Original,
                            direction: EntryDirection::Increase,
                            amount: Amount::from_str("100.00").unwrap(),
                            due_date: BusinessDate::from_ymd(2026, 9, 30).unwrap(),
                            source_fact_type: "sales_order".to_string(),
                            source_document_id: "so-1".to_string(),
                            source_revision_id: "sor-1".to_string(),
                            source_sequence: 1,
                            posted_at: Instant::from_unix_secs(1_700_000_000),
                        },
                    )
                    .unwrap(),
                    &mut NoTransaction,
                )
                .await
                .expect("分录写入失败");
            db.receipt_allocations()
                .create(
                    &ReceiptAllocation::new(
                        ReceiptAllocationId::new("al-1"),
                        ReceiptAllocationData {
                            customer_receipt_id: CustomerReceiptId::new("missing-cr"),
                            receivable_entry_id: ReceivableEntryId::new("e-1"),
                            allocation_seq: 1,
                            allocation_action: AllocationAction::Apply,
                            allocated_amount: Amount::from_str("10.00").unwrap(),
                            allocated_at: Instant::from_unix_secs(1_700_000_000),
                            reverses_allocation_id: None,
                        },
                    )
                    .unwrap(),
                    &mut NoTransaction,
                )
                .await
                .expect("回款分配写入失败");
            db.sales_invoice_allocations()
                .create(
                    &SalesInvoiceAllocation::new(
                        SalesInvoiceAllocationId::new("sia-1"),
                        SalesInvoiceAllocationData {
                            invoice_id: InvoiceId::new("missing-inv"),
                            receivable_account_id: ReceivableAccountId::new("ra-1"),
                            allocation_seq: 1,
                            allocation_action: AllocationAction::Apply,
                            allocated_gross_amount: Amount::from_str("10.00").unwrap(),
                            allocated_net_amount: Amount::from_str("10.00").unwrap(),
                            allocated_tax_amount: Amount::from_str("0.00").unwrap(),
                            reverses_allocation_id: None,
                        },
                    )
                    .unwrap(),
                    &mut NoTransaction,
                )
                .await
                .expect("发票分配写入失败");

            let facts = db
                .receivable()
                .receivable_snapshot_facts(&ReceivableAccountId::new("ra-1"), &mut NoTransaction)
                .await
                .expect("快照读取失败");
            assert_eq!(facts.expected_receipt_count, 1);
            assert!(facts.receipts.is_empty());
            assert_eq!(facts.expected_invoice_count, 1);
            assert!(facts.invoices.is_empty());
            assert!(facts.receipts.len() != facts.expected_receipt_count);
            assert!(facts.invoices.len() != facts.expected_invoice_count);
        });
    }

    #[tokio::test]
    #[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
    async fn snapshot_facts_missing_receipts_only_keeps_invoice_count_ok() {
        use erp_core::common::time::{BusinessDate, Instant};
        use erp_core::ids::{CustomerReceiptId, ReceiptAllocationId, ReceivableEntryId};

        use crate::entity::receivable::{
            AllocationAction, EntryDirection, ReceiptAllocation, ReceiptAllocationData, ReceivableEntry,
            ReceivableEntryData, ReceivableEntryType,
        };
        use crate::repository::test_fixture::{TestDb, require_mongo};

        require_mongo!(async {
            let fixture = TestDb::new("recv_snapshot_receipt_only").await.expect("测试数据库创建失败");
            crate::indexes::ensure(fixture.db()).await.expect("索引创建失败");
            let db = fixture.db();
            db.receivable_accounts()
                .create(&test_account("ra-1", "100.00", "0.00"), &mut NoTransaction)
                .await
                .expect("子账写入失败");
            db.receivable_entries()
                .create(
                    &ReceivableEntry::new(
                        ReceivableEntryId::new("e-1"),
                        ReceivableEntryData {
                            receivable_account_id: ReceivableAccountId::new("ra-1"),
                            entry_type: ReceivableEntryType::Original,
                            direction: EntryDirection::Increase,
                            amount: Amount::from_str("100.00").unwrap(),
                            due_date: BusinessDate::from_ymd(2026, 9, 30).unwrap(),
                            source_fact_type: "sales_order".to_string(),
                            source_document_id: "so-1".to_string(),
                            source_revision_id: "sor-1".to_string(),
                            source_sequence: 1,
                            posted_at: Instant::from_unix_secs(1_700_000_000),
                        },
                    )
                    .unwrap(),
                    &mut NoTransaction,
                )
                .await
                .expect("分录写入失败");
            db.receipt_allocations()
                .create(
                    &ReceiptAllocation::new(
                        ReceiptAllocationId::new("al-1"),
                        ReceiptAllocationData {
                            customer_receipt_id: CustomerReceiptId::new("missing-cr"),
                            receivable_entry_id: ReceivableEntryId::new("e-1"),
                            allocation_seq: 1,
                            allocation_action: AllocationAction::Apply,
                            allocated_amount: Amount::from_str("10.00").unwrap(),
                            allocated_at: Instant::from_unix_secs(1_700_000_000),
                            reverses_allocation_id: None,
                        },
                    )
                    .unwrap(),
                    &mut NoTransaction,
                )
                .await
                .expect("回款分配写入失败");
            let facts = db
                .receivable()
                .receivable_snapshot_facts(&ReceivableAccountId::new("ra-1"), &mut NoTransaction)
                .await
                .expect("快照读取失败");
            assert_eq!(facts.expected_receipt_count, 1);
            assert!(facts.receipts.is_empty());
            assert_eq!(facts.expected_invoice_count, 0);
        });
    }

    fn receipt_allocation(
        id: &str,
        receipt_id: &str,
        entry_id: &str,
        seq: u32,
        value: &str,
    ) -> crate::entity::receivable::ReceiptAllocation {
        use erp_core::common::time::Instant;
        use erp_core::ids::{CustomerReceiptId, ReceiptAllocationId, ReceivableEntryId};

        use crate::entity::receivable::{AllocationAction, ReceiptAllocation, ReceiptAllocationData};
        ReceiptAllocation::new(
            ReceiptAllocationId::new(id),
            ReceiptAllocationData {
                customer_receipt_id: CustomerReceiptId::new(receipt_id),
                receivable_entry_id: ReceivableEntryId::new(entry_id),
                allocation_seq: seq,
                allocation_action: AllocationAction::Apply,
                allocated_amount: Amount::from_str(value).unwrap(),
                allocated_at: Instant::from_unix_secs(1_700_000_000),
                reverses_allocation_id: None,
            },
        )
        .unwrap()
    }

    #[tokio::test]
    #[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
    async fn historical_and_approval_batch_writes_conserve_and_rollback() {
        use persistence_core::Transactional;

        use crate::repository::test_fixture::{TestDb, require_mongo};

        require_mongo!(async {
            let fixture = TestDb::new("recv_receipt_batch").await.expect("测试数据库创建失败");
            crate::indexes::ensure(fixture.db()).await.expect("索引创建失败");
            let db = fixture.db();
            db.receivable_accounts()
                .create(&test_account("acct-1", "1000.00", "0.00"), &mut NoTransaction)
                .await
                .expect("子账写入失败");

            // 历史登记数据面：同一账户聚合一次条件核销 + insert_many。
            let historical = [
                receipt_allocation("h-1", "cr-h", "e-1", 1, "40.00"),
                receipt_allocation("h-2", "cr-h", "e-2", 2, "60.00"),
            ];
            let hist_delta = [(ReceivableAccountId::new("acct-1"), Amount::from_str("100.00").unwrap())];
            let hist_result = db
                .receivable_accounts()
                .apply_settlements_many(&hist_delta, "tester", &mut NoTransaction)
                .await
                .expect("历史核销失败");
            assert!(hist_result.rejected.is_empty());
            db.receivable()
                .create_receipt_allocations_many(&historical, &mut NoTransaction)
                .await
                .expect("历史分配写入失败");
            let after_hist = db
                .receivable_accounts()
                .find_by_id("acct-1", &mut NoTransaction)
                .await
                .expect("读取失败")
                .expect("子账必须存在");
            assert_eq!(after_hist.settled_total, Amount::from_str("100.00").unwrap());
            assert_eq!(after_hist.open_total, Amount::from_str("900.00").unwrap());

            // 审批过账数据面：同账户两行分配一次聚合推进。
            let approval = [
                receipt_allocation("a-1", "cr-a", "e-1", 1, "30.00"),
                receipt_allocation("a-2", "cr-a", "e-1", 2, "20.00"),
            ];
            let approval_delta = [(ReceivableAccountId::new("acct-1"), Amount::from_str("50.00").unwrap())];
            let approval_result = db
                .receivable_accounts()
                .apply_settlements_many(&approval_delta, "tester", &mut NoTransaction)
                .await
                .expect("审批核销失败");
            assert!(approval_result.rejected.is_empty());
            db.receivable()
                .create_receipt_allocations_many(&approval, &mut NoTransaction)
                .await
                .expect("审批分配写入失败");
            let after_post = db
                .receivable_accounts()
                .find_by_id("acct-1", &mut NoTransaction)
                .await
                .expect("读取失败")
                .expect("子账必须存在");
            assert_eq!(after_post.settled_total, Amount::from_str("150.00").unwrap());
            assert_eq!(after_post.open_total, Amount::from_str("850.00").unwrap());

            // 唯一键冲突整事务回滚：进度与分配零残留。
            let db_tx = db.clone();
            let outcome = fixture.client().with_transaction(move |session| {
                let db_tx = db_tx.clone();
                Box::pin(async move {
                    let delta = [(ReceivableAccountId::new("acct-1"), Amount::from_str("10.00").unwrap())];
                    let applied =
                        db_tx.receivable_accounts().apply_settlements_many(&delta, "tester", session).await?;
                    if !applied.rejected.is_empty() {
                        return Err(persistence_core::Error::OptimisticLockingError);
                    }
                    db_tx
                        .receivable()
                        .create_receipt_allocations_many(
                            &[
                                receipt_allocation("dup-1", "cr-h", "e-1", 1, "10.00"),
                                receipt_allocation("dup-2", "cr-h", "e-1", 1, "10.00"),
                            ],
                            session,
                        )
                        .await?;
                    Ok(())
                })
            });
            assert!(outcome.await.is_err(), "重复序号必须失败");
            let after_conflict = db
                .receivable_accounts()
                .find_by_id("acct-1", &mut NoTransaction)
                .await
                .expect("读取失败")
                .expect("子账必须存在");
            assert_eq!(
                after_conflict.settled_total,
                Amount::from_str("150.00").unwrap(),
                "唯一键冲突必须回滚核销进度"
            );
        });
    }

    #[tokio::test]
    #[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
    async fn concurrent_settlement_same_account_exactly_one_hits() {
        use crate::repository::test_fixture::{TestDb, require_mongo};

        require_mongo!(async {
            let fixture = TestDb::new("recv_settle_concurrent").await.expect("测试数据库创建失败");
            crate::indexes::ensure(fixture.db()).await.expect("索引创建失败");
            fixture
                .db()
                .receivable_accounts()
                .create(&test_account("acct-1", "100.00", "0.00"), &mut NoTransaction)
                .await
                .expect("子账写入失败");
            let deltas = vec![(ReceivableAccountId::new("acct-1"), Amount::from_str("80.00").unwrap())];
            let db_a = fixture.db().clone();
            let deltas_a = deltas.clone();
            let task_a = tokio::spawn(async move {
                db_a.receivable_accounts()
                    .apply_settlements_many(&deltas_a, "tester-a", &mut NoTransaction)
                    .await
                    .expect("写入方 A 失败")
            });
            let db_b = fixture.db().clone();
            let task_b = tokio::spawn(async move {
                db_b.receivable_accounts()
                    .apply_settlements_many(&deltas, "tester-b", &mut NoTransaction)
                    .await
                    .expect("写入方 B 失败")
            });
            let result_a = task_a.await.expect("任务 A 失败");
            let result_b = task_b.await.expect("任务 B 失败");
            assert_eq!(result_a.applied.len() + result_b.applied.len(), 1, "80+80 超 100 时恰好一方命中");
            assert_eq!(result_a.rejected.len() + result_b.rejected.len(), 1);
            let after = fixture
                .db()
                .receivable_accounts()
                .find_by_id("acct-1", &mut NoTransaction)
                .await
                .expect("读取失败")
                .expect("子账必须存在");
            assert_eq!(after.settled_total, Amount::from_str("80.00").unwrap());
            assert_eq!(after.open_total, Amount::from_str("20.00").unwrap());
        });
    }
}
