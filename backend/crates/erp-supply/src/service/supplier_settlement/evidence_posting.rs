//! 差异补证的本域复验与写入；不改变 prepare 阶段的身份、ID 或时钟。

use async_trait::async_trait;
use mongodb::Database;
use persistence_core::Executor;

use crate::entity::supplier_settlement::{
    SettlementStatus, SupplierSettlementDifference, SupplierSettlementDifferenceEvidence,
    SupplierSettlementItem, SupplierSettlementStatement,
};
use crate::repository::SupplierSettlementExt;
use crate::{Error, Result};

/// 本域补证的五个实际 I/O 步骤；存在性与状态首错由唯一 runner 决定。
#[async_trait]
trait EvidenceStore: Send {
    async fn difference(
        &mut self,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<SupplierSettlementDifference>>;
    async fn item(&mut self, id: &str, executor: &mut dyn Executor)
    -> Result<Option<SupplierSettlementItem>>;
    async fn statement(
        &mut self,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<SupplierSettlementStatement>>;
    async fn update_statement(
        &mut self,
        statement: &mut SupplierSettlementStatement,
        executor: &mut dyn Executor,
    ) -> Result<()>;
    async fn create_evidence(
        &mut self,
        evidence: &SupplierSettlementDifferenceEvidence,
        executor: &mut dyn Executor,
    ) -> Result<()>;
}

/// 沿原 owned 仓储与调用方执行器实施读取和 CAS，不创建事务。
struct MongoEvidenceStore<'a> {
    db: &'a Database,
}

#[async_trait]
impl EvidenceStore for MongoEvidenceStore<'_> {
    async fn difference(
        &mut self,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<SupplierSettlementDifference>> {
        Ok(self.db.supplier_settlement_differences().find_by_id(id, executor).await?)
    }
    async fn item(
        &mut self,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<SupplierSettlementItem>> {
        Ok(self.db.supplier_settlement_items().find_by_id(id, executor).await?)
    }
    async fn statement(
        &mut self,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<SupplierSettlementStatement>> {
        Ok(self.db.supplier_settlement_statements().find_by_id(id, executor).await?)
    }
    async fn update_statement(
        &mut self,
        statement: &mut SupplierSettlementStatement,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        self.db.supplier_settlement_statements().update(statement, executor).await?;
        Ok(())
    }
    async fn create_evidence(
        &mut self,
        evidence: &SupplierSettlementDifferenceEvidence,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        self.db.supplier_settlement_difference_evidence().create(evidence, executor).await?;
        Ok(())
    }
}

/// 接通真实数据库 provider；准备好的补证与所有身份按原参数传入。
pub(super) async fn persist(
    db: &Database,
    difference_id: &str,
    statement_id: &str,
    expected_version: u64,
    evidence: &SupplierSettlementDifferenceEvidence,
    executor: &mut dyn Executor,
) -> Result<()> {
    persist_with_store(
        &mut MongoEvidenceStore { db },
        difference_id,
        statement_id,
        expected_version,
        evidence,
        executor,
    )
    .await
}

/// 先复验差异、明细和结算头，再 CAS 结算主题并追加补证；首错停止。
async fn persist_with_store<S: EvidenceStore>(
    store: &mut S,
    difference_id: &str,
    statement_id: &str,
    expected_version: u64,
    evidence: &SupplierSettlementDifferenceEvidence,
    executor: &mut dyn Executor,
) -> Result<()> {
    let current = store
        .difference(difference_id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("供应商结算差异不存在".to_string()))?;
    if current.base.version != expected_version {
        return Err(Error::ConflictError("结算差异版本已变化，请刷新后重试".to_string()));
    }
    let item = store
        .item(current.statement_item_id.as_ref(), executor)
        .await?
        .ok_or_else(|| Error::NotFound("结算差异所属明细不存在".to_string()))?;
    if item.statement_id.as_ref() != statement_id {
        return Err(Error::BusinessLogicError("差异不属于命令指定的结算单".to_string()));
    }
    let mut statement = store
        .statement(statement_id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("供应商结算单不存在".to_string()))?;
    if !matches!(
        statement.status,
        SettlementStatus::Draft | SettlementStatus::PendingReconciliation | SettlementStatus::HasDifference
    ) {
        return Err(Error::BusinessLogicError("当前结算状态禁止追加差异补证".to_string()));
    }
    // 补证属于结算主题变更：与提交复核共同 CAS 同一结算单，禁止
    // `PENDING_REVIEW` 状态推进和迟到证据在不同文档上并发穿透。
    store.update_statement(&mut statement, executor).await?;
    store.create_evidence(evidence, executor).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use erp_core::common::time::{BusinessDate, Instant};
    use erp_core::ids::{
        SupplierAccountId, SupplierFulfillmentItemId, SupplierFulfillmentOrderId,
        SupplierSettlementDifferenceId, SupplierSettlementItemId, SupplierSettlementStatementId,
    };
    use erp_core::money::{Amount, Quantity};

    use super::*;
    use crate::entity::supplier_settlement::{
        SettlementDifferenceStatus, SettlementDifferenceType, SupplierSettlementDifferenceData,
        SupplierSettlementDifferenceEvidenceData, SupplierSettlementItemData,
        SupplierSettlementStatementData,
    };

    struct TestExecutor {
        _identity: u8,
    }
    impl Executor for TestExecutor {
        fn session(&mut self) -> Option<&mut mongodb::ClientSession> {
            None
        }
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum Step {
        Difference,
        Item,
        Statement,
        StatementCas,
        EvidenceCreate,
    }

    struct Store {
        difference: Option<SupplierSettlementDifference>,
        item: Option<SupplierSettlementItem>,
        statement: Option<SupplierSettlementStatement>,
        fail: Option<Step>,
        calls: Vec<(Step, String)>,
        executors: Vec<usize>,
        written_statement: Option<SupplierSettlementStatement>,
        written_evidence: Option<SupplierSettlementDifferenceEvidence>,
    }

    impl Store {
        fn new(status: SettlementStatus) -> Self {
            let mut difference =
                difference_fixture("difference-1", "item-1", SettlementDifferenceStatus::Pending, 1);
            difference.base.version = 7;
            let mut statement = statement_fixture("statement-1");
            statement.status = status;
            Self {
                difference: Some(difference),
                item: Some(item_fixture("item-1", "statement-1", 1)),
                statement: Some(statement),
                fail: None,
                calls: Vec::new(),
                executors: Vec::new(),
                written_statement: None,
                written_evidence: None,
            }
        }
        fn record(&mut self, step: Step, id: &str, executor: &mut dyn Executor) -> Result<()> {
            self.calls.push((step, id.to_string()));
            self.executors.push(executor as *mut dyn Executor as *mut () as usize);
            if self.fail == Some(step) {
                return Err(Error::Forbidden(format!("io:{step:?}")));
            }
            Ok(())
        }
    }

    #[async_trait]
    impl EvidenceStore for Store {
        async fn difference(
            &mut self,
            id: &str,
            executor: &mut dyn Executor,
        ) -> Result<Option<SupplierSettlementDifference>> {
            self.record(Step::Difference, id, executor)?;
            Ok(self.difference.clone())
        }
        async fn item(
            &mut self,
            id: &str,
            executor: &mut dyn Executor,
        ) -> Result<Option<SupplierSettlementItem>> {
            self.record(Step::Item, id, executor)?;
            Ok(self.item.clone())
        }
        async fn statement(
            &mut self,
            id: &str,
            executor: &mut dyn Executor,
        ) -> Result<Option<SupplierSettlementStatement>> {
            self.record(Step::Statement, id, executor)?;
            Ok(self.statement.clone())
        }
        async fn update_statement(
            &mut self,
            statement: &mut SupplierSettlementStatement,
            executor: &mut dyn Executor,
        ) -> Result<()> {
            self.record(Step::StatementCas, &statement.base.id, executor)?;
            statement.base.version += 1;
            self.written_statement = Some(statement.clone());
            Ok(())
        }
        async fn create_evidence(
            &mut self,
            evidence: &SupplierSettlementDifferenceEvidence,
            executor: &mut dyn Executor,
        ) -> Result<()> {
            self.record(Step::EvidenceCreate, &evidence.base.id, executor)?;
            assert!(self.written_statement.is_some());
            self.written_evidence = Some(evidence.clone());
            Ok(())
        }
    }

    fn expected_trace() -> Vec<(Step, String)> {
        [
            (Step::Difference, "difference-1"),
            (Step::Item, "item-1"),
            (Step::Statement, "statement-1"),
            (Step::StatementCas, "statement-1"),
            (Step::EvidenceCreate, "evidence-1"),
        ]
        .into_iter()
        .map(|(step, id)| (step, id.to_string()))
        .collect()
    }

    fn assert_trace(store: &Store, count: usize, identity: usize) {
        assert_eq!(store.calls, expected_trace()[..count]);
        assert_eq!(store.executors, vec![identity; count]);
    }

    #[tokio::test]
    async fn evidence_posting_reads_before_statement_cas_and_evidence_on_same_executor() {
        for status in [
            SettlementStatus::Draft,
            SettlementStatus::PendingReconciliation,
            SettlementStatus::HasDifference,
        ] {
            let mut store = Store::new(status);
            let version = store.statement.as_ref().unwrap().base.version;
            let evidence = sample_evidence("evidence-1", "difference-1", 1700000000);
            let mut executor = TestExecutor { _identity: 1 };
            let identity = &mut executor as *mut TestExecutor as usize;
            persist_with_store(&mut store, "difference-1", "statement-1", 7, &evidence, &mut executor)
                .await
                .unwrap();
            assert_trace(&store, 5, identity);
            assert_eq!(store.written_statement.as_ref().unwrap().base.version, version + 1);
            assert_eq!(store.written_evidence.as_ref(), Some(&evidence));
        }
    }

    #[tokio::test]
    async fn evidence_posting_preserves_every_io_error_and_stops_at_its_prefix() {
        for (index, (step, _)) in expected_trace().iter().enumerate() {
            let mut store = Store::new(SettlementStatus::Draft);
            store.fail = Some(*step);
            let evidence = sample_evidence("evidence-1", "difference-1", 1700000000);
            let mut executor = TestExecutor { _identity: 2 };
            let identity = &mut executor as *mut TestExecutor as usize;
            let result =
                persist_with_store(&mut store, "difference-1", "statement-1", 7, &evidence, &mut executor)
                    .await;
            assert!(matches!(result,Err(Error::Forbidden(message)) if message==format!("io:{step:?}")));
            assert_trace(&store, index + 1, identity);
            assert!(store.written_evidence.is_none());
        }
    }

    #[tokio::test]
    async fn evidence_posting_missing_facts_fail_at_the_original_read() {
        for (missing, message) in
            [(0, "供应商结算差异不存在"), (1, "结算差异所属明细不存在"), (2, "供应商结算单不存在")]
        {
            let mut store = Store::new(SettlementStatus::Draft);
            match missing {
                0 => store.difference = None,
                1 => store.item = None,
                _ => store.statement = None,
            }
            let evidence = sample_evidence("evidence-1", "difference-1", 1700000000);
            let mut executor = TestExecutor { _identity: 3 };
            let identity = &mut executor as *mut TestExecutor as usize;
            let result =
                persist_with_store(&mut store, "difference-1", "statement-1", 7, &evidence, &mut executor)
                    .await;
            assert!(matches!(result,Err(Error::NotFound(actual)) if actual==message));
            assert_trace(&store, missing + 1, identity);
        }
    }

    #[tokio::test]
    async fn evidence_posting_version_precedes_association_and_association_precedes_state() {
        for stale in [true, false] {
            let mut store = Store::new(SettlementStatus::PendingReview);
            store.item.as_mut().unwrap().statement_id = SupplierSettlementStatementId::new("other-statement");
            let evidence = sample_evidence("evidence-1", "difference-1", 1700000000);
            let mut executor = TestExecutor { _identity: 4 };
            let identity = &mut executor as *mut TestExecutor as usize;
            let result = persist_with_store(
                &mut store,
                "difference-1",
                "statement-1",
                if stale { 6 } else { 7 },
                &evidence,
                &mut executor,
            )
            .await;
            if stale {
                assert!(
                    matches!(result,Err(Error::ConflictError(message)) if message=="结算差异版本已变化，请刷新后重试")
                );
                assert_trace(&store, 1, identity);
            } else {
                assert!(
                    matches!(result,Err(Error::BusinessLogicError(message)) if message=="差异不属于命令指定的结算单")
                );
                assert_trace(&store, 2, identity);
            }
            assert!(store.written_statement.is_none());
            assert!(store.written_evidence.is_none());
        }
    }

    #[tokio::test]
    async fn evidence_posting_noneditable_state_stops_before_both_writes() {
        for status in [SettlementStatus::PendingReview, SettlementStatus::Confirmed, SettlementStatus::Voided]
        {
            let mut store = Store::new(status);
            let evidence = sample_evidence("evidence-1", "difference-1", 1700000000);
            let mut executor = TestExecutor { _identity: 5 };
            let identity = &mut executor as *mut TestExecutor as usize;
            let result =
                persist_with_store(&mut store, "difference-1", "statement-1", 7, &evidence, &mut executor)
                    .await;
            assert!(
                matches!(result,Err(Error::BusinessLogicError(message)) if message=="当前结算状态禁止追加差异补证")
            );
            assert_trace(&store, 3, identity);
            assert!(store.written_statement.is_none());
            assert!(store.written_evidence.is_none());
        }
    }

    /// 构造结算单夹具。
    ///
    /// # 参数
    /// * `id` - 结算单主键
    ///
    /// # 返回
    /// 返回草稿状态的结算单实体。
    fn statement_fixture(id: &str) -> SupplierSettlementStatement {
        SupplierSettlementStatement::new(
            SupplierSettlementStatementId::new(id),
            SupplierSettlementStatementData {
                statement_no: format!("ST-{id}"),
                supplier_id: SupplierAccountId::new("supplier-1"),
                period_start: BusinessDate::from_ymd(2026, 7, 1).unwrap(),
                period_end: BusinessDate::from_ymd(2026, 7, 31).unwrap(),
                period_policy_id: "calendar-month".to_string(),
                period_policy_version: "1".to_string(),
                period_timezone: "Asia/Shanghai".to_string(),
                external_bill_no: Some(format!("BILL-{id}")),
                external_bill_version: Some("1".to_string()),
                erp_amount: Amount::from_str("115.00").unwrap(),
                supplier_amount: Amount::from_str("115.00").unwrap(),
                subject_hash: "a".repeat(64),
                source_as_of: Instant::from_unix_secs(1_700_000_000),
                source_snapshot_at: Instant::from_unix_secs(1_700_000_000),
                source_snapshot_hash: "b".repeat(64),
                refresh_cutoff_policy_id: "supplier-settlement-review-cutoff".to_string(),
                refresh_cutoff_policy_version: "1".to_string(),
                prepared_by: "preparer-1".to_string(),
            },
        )
        .unwrap()
    }
    /// 构造结算明细夹具（订单 100 + 运费 10 + 服务费 5 − 退款 0 = ERP 115）。
    ///
    /// # 参数
    /// * `id` - 明细主键
    /// * `statement_id` - 所属结算单主键
    /// * `created_at` - 冻结创建时间（覆盖时钟，便于排序断言）
    ///
    /// # 返回
    /// 返回满足金额恒等的冻结明细实体。
    fn item_fixture(id: &str, statement_id: &str, created_at: u64) -> SupplierSettlementItem {
        let mut item = SupplierSettlementItem::new(
            SupplierSettlementItemId::new(id),
            SupplierSettlementItemData {
                statement_id: SupplierSettlementStatementId::new(statement_id),
                supplier_fulfillment_order_id: SupplierFulfillmentOrderId::new(format!("order-{id}")),
                supplier_fulfillment_item_id: SupplierFulfillmentItemId::new(format!("fulfillment-{id}")),
                quantity: Quantity::from_str("1").unwrap(),
                order_amount: Amount::from_str("100.00").unwrap(),
                freight_amount: Amount::from_str("10.00").unwrap(),
                service_fee_amount: Amount::from_str("5.00").unwrap(),
                refund_amount: Amount::from_str("0.00").unwrap(),
                erp_calculated_amount: Amount::from_str("115.00").unwrap(),
                erp_calculated_net_amount: Amount::from_str("100.00").unwrap(),
                erp_calculated_tax_amount: Amount::from_str("15.00").unwrap(),
                supplier_billed_amount: Amount::from_str("115.00").unwrap(),
                supplier_billed_net_amount: Amount::from_str("100.00").unwrap(),
                supplier_billed_tax_amount: Amount::from_str("15.00").unwrap(),
            },
        )
        .unwrap();
        item.base.created_at = created_at;
        item
    }
    /// 构造结算差异夹具。
    ///
    /// # 参数
    /// * `id` - 差异主键
    /// * `item_id` - 所属明细主键
    /// * `status` - 差异状态（待处理或已认可不带处理三元组）
    /// * `created_at` - 创建时间（覆盖时钟，便于排序断言）
    ///
    /// # 返回
    /// 返回新建的结算差异实体。
    fn difference_fixture(
        id: &str,
        item_id: &str,
        status: SettlementDifferenceStatus,
        created_at: u64,
    ) -> SupplierSettlementDifference {
        let mut difference = SupplierSettlementDifference::new(
            SupplierSettlementDifferenceId::new(id),
            SupplierSettlementDifferenceData {
                statement_item_id: SupplierSettlementItemId::new(item_id),
                difference_type: SettlementDifferenceType::Amount,
                difference_amount: Amount::from_str("1.00").unwrap(),
                status,
                resolution: None,
                resolved_by: None,
                resolved_at: None,
            },
        )
        .unwrap();
        difference.base.created_at = created_at;
        difference
    }
    /// 构造单条补证实体。
    ///
    /// # 参数
    /// * `statement_id` - 所属结算单主键
    /// * `id` - 补证主键
    /// * `difference_id` - 所属差异主键
    /// * `provided_at_secs` - 补证时间（秒级时间戳）
    ///
    /// # 返回
    /// 返回可直接归组的不可变补证实体。
    fn sample_evidence_in_statement(
        statement_id: &str,
        id: &str,
        difference_id: &str,
        provided_at_secs: i64,
    ) -> SupplierSettlementDifferenceEvidence {
        SupplierSettlementDifferenceEvidence::new(
            id,
            SupplierSettlementDifferenceEvidenceData {
                request_id: format!("request-{id}"),
                statement_id: SupplierSettlementStatementId::new(statement_id),
                difference_id: SupplierSettlementDifferenceId::new(difference_id),
                evidence_reference_ids: vec![format!("ticket://{id}")],
                opinion_code: None,
                comment: None,
                provided_by: "preparer-1".to_string(),
                provided_at: Instant::from_unix_secs(provided_at_secs),
                command_hash: "a".repeat(64),
            },
        )
        .unwrap()
    }
    /// 构造单条补证实体（归属 `statement-1`）。
    ///
    /// # 参数
    /// * `id` - 补证主键
    /// * `difference_id` - 所属差异主键
    /// * `provided_at_secs` - 补证时间（秒级时间戳）
    ///
    /// # 返回
    /// 返回可直接归组的不可变补证实体。
    fn sample_evidence(
        id: &str,
        difference_id: &str,
        provided_at_secs: i64,
    ) -> SupplierSettlementDifferenceEvidence {
        sample_evidence_in_statement("statement-1", id, difference_id, provided_at_secs)
    }
}
