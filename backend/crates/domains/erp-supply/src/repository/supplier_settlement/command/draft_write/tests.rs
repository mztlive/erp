//! 真实草稿替换 runner 的调用顺序与错误前缀，不连接 MongoDB。
use std::str::FromStr;

use erp_core::common::time::{BusinessDate, Instant};
use erp_core::ids::{
    SupplierAccountId, SupplierFulfillmentItemId, SupplierFulfillmentOrderId, SupplierSettlementDifferenceId,
    SupplierSettlementItemId, SupplierSettlementStatementId,
};
use erp_core::money::{Amount, Quantity};

use super::*;
use crate::entity::supplier_settlement::{
    SettlementDifferenceStatus, SettlementDifferenceType, SettlementStatus, SupplierSettlementDifferenceData,
    SupplierSettlementItemData, SupplierSettlementStatementData,
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
    DeleteEvidence,
    DeleteDifferences,
    DeleteItems,
    UpdateStatement,
    InsertItems,
    InsertDifferences,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Call {
    step: Step,
    ids: Vec<String>,
}

struct Store {
    fail: Option<Step>,
    calls: Vec<Call>,
    executors: Vec<usize>,
}

impl Store {
    fn new(fail: Option<Step>) -> Self {
        Self { fail, calls: Vec::new(), executors: Vec::new() }
    }
    fn record(&mut self, step: Step, ids: Vec<String>, executor: &mut dyn Executor) -> Result<()> {
        self.calls.push(Call { step, ids });
        self.executors.push(executor as *mut dyn Executor as *mut () as usize);
        if self.fail == Some(step) {
            return Err(persistence_core::Error::OptimisticLockingError);
        }
        Ok(())
    }
}

#[async_trait]
impl DraftSnapshotStore for Store {
    async fn delete_evidence(&mut self, ids: &[String], executor: &mut dyn Executor) -> Result<()> {
        self.record(Step::DeleteEvidence, ids.to_vec(), executor)
    }
    async fn delete_differences(&mut self, ids: &[String], executor: &mut dyn Executor) -> Result<()> {
        self.record(Step::DeleteDifferences, ids.to_vec(), executor)
    }
    async fn delete_items(&mut self, id: &str, executor: &mut dyn Executor) -> Result<()> {
        self.record(Step::DeleteItems, vec![id.to_string()], executor)
    }
    async fn update_statement(
        &mut self,
        statement: &mut SupplierSettlementStatement,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        self.record(Step::UpdateStatement, vec![statement.base.id.clone()], executor)?;
        statement.base.version += 1;
        Ok(())
    }
    async fn insert_items(
        &mut self,
        items: &[SupplierSettlementItem],
        executor: &mut dyn Executor,
    ) -> Result<()> {
        self.record(Step::InsertItems, items.iter().map(|item| item.base.id.clone()).collect(), executor)
    }
    async fn insert_differences(
        &mut self,
        differences: &[SupplierSettlementDifference],
        executor: &mut dyn Executor,
    ) -> Result<()> {
        self.record(
            Step::InsertDifferences,
            differences.iter().map(|difference| difference.base.id.clone()).collect(),
            executor,
        )
    }
}

fn call(step: Step, ids: &[&str]) -> Call {
    Call { step, ids: ids.iter().map(|id| id.to_string()).collect() }
}

fn full_trace() -> Vec<Call> {
    vec![
        call(Step::DeleteEvidence, &["old-difference-b", "old-difference-a"]),
        call(Step::DeleteDifferences, &["old-item-b", "old-item-a"]),
        call(Step::DeleteItems, &["statement-1"]),
        call(Step::UpdateStatement, &["statement-1"]),
        call(Step::InsertItems, &["new-item-b", "new-item-a"]),
        call(Step::InsertDifferences, &["new-difference"]),
    ]
}

fn replacement() -> (Vec<SupplierSettlementItem>, Vec<SupplierSettlementDifference>) {
    (
        vec![item_fixture("new-item-b", "statement-1", 2), item_fixture("new-item-a", "statement-1", 1)],
        vec![difference_fixture("new-difference", "new-item-a", SettlementDifferenceStatus::Pending, 1)],
    )
}

#[tokio::test]
async fn draft_replace_keeps_physical_delete_cas_insert_order_and_one_executor() {
    let mut statement = statement_fixture("statement-1");
    let version = statement.base.version;
    let (items, differences) = replacement();
    let mut store = Store::new(None);
    let mut executor = TestExecutor { _identity: 1 };
    let identity = &mut executor as *mut TestExecutor as usize;
    replace_snapshot(
        &mut store,
        &mut statement,
        &["old-item-b".into(), "old-item-a".into()],
        &["old-difference-b".into(), "old-difference-a".into()],
        &items,
        &differences,
        &mut executor,
    )
    .await
    .unwrap();
    assert_eq!(store.calls, full_trace());
    assert_eq!(store.executors, vec![identity; 6]);
    assert_eq!(statement.base.version, version + 1);
}

#[tokio::test]
async fn draft_replace_stops_at_every_failed_write_and_keeps_original_error() {
    let expected = full_trace();
    for (index, failed) in expected.iter().enumerate() {
        let mut statement = statement_fixture("statement-1");
        let (items, differences) = replacement();
        let mut store = Store::new(Some(failed.step));
        let mut executor = TestExecutor { _identity: 2 };
        let identity = &mut executor as *mut TestExecutor as usize;
        let result = replace_snapshot(
            &mut store,
            &mut statement,
            &["old-item-b".into(), "old-item-a".into()],
            &["old-difference-b".into(), "old-difference-a".into()],
            &items,
            &differences,
            &mut executor,
        )
        .await;
        assert!(matches!(result, Err(persistence_core::Error::OptimisticLockingError)));
        assert_eq!(store.calls, expected[..=index]);
        assert_eq!(store.executors, vec![identity; index + 1]);
    }
}

#[tokio::test]
async fn draft_replace_keeps_original_empty_batch_conditions() {
    for has_old_item in [false, true] {
        for has_old_difference in [false, true] {
            for has_new_difference in [false, true] {
                let mut statement = statement_fixture("statement-1");
                let old_items = if has_old_item { vec!["old-item".to_string()] } else { Vec::new() };
                let old_differences =
                    if has_old_difference { vec!["old-difference".to_string()] } else { Vec::new() };
                let differences = if has_new_difference {
                    vec![difference_fixture(
                        "new-difference",
                        "new-item",
                        SettlementDifferenceStatus::Pending,
                        1,
                    )]
                } else {
                    Vec::new()
                };
                let mut store = Store::new(None);
                let mut executor = TestExecutor { _identity: 3 };
                let identity = &mut executor as *mut TestExecutor as usize;
                replace_snapshot(
                    &mut store,
                    &mut statement,
                    &old_items,
                    &old_differences,
                    &[],
                    &differences,
                    &mut executor,
                )
                .await
                .unwrap();
                let mut expected = Vec::new();
                if has_old_difference {
                    expected.push(call(Step::DeleteEvidence, &["old-difference"]));
                }
                if has_old_item {
                    expected.push(call(Step::DeleteDifferences, &["old-item"]));
                }
                expected.extend([
                    call(Step::DeleteItems, &["statement-1"]),
                    call(Step::UpdateStatement, &["statement-1"]),
                    call(Step::InsertItems, &[]),
                ]);
                if has_new_difference {
                    expected.push(call(Step::InsertDifferences, &["new-difference"]));
                }
                assert_eq!(store.calls, expected);
                assert_eq!(store.executors, vec![identity; expected.len()]);
            }
        }
    }
}

#[tokio::test]
async fn draft_store_does_not_add_a_state_guard_before_original_first_write() {
    let mut statement = statement_fixture("statement-1");
    statement.status = SettlementStatus::Confirmed;
    let mut store = Store::new(Some(Step::DeleteEvidence));
    let mut executor = TestExecutor { _identity: 4 };
    let result = replace_snapshot(
        &mut store,
        &mut statement,
        &["old-item".into()],
        &["old-difference".into()],
        &[],
        &[],
        &mut executor,
    )
    .await;
    assert!(matches!(result, Err(persistence_core::Error::OptimisticLockingError)));
    assert_eq!(store.calls, [call(Step::DeleteEvidence, &["old-difference"])]);
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
            business_org_unit_id: "org-finance".to_string(),
            difference_handler_user_id: String::new(),
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
