//! 销售单回款/开票进度与关闭状态的派生刷新（§9.3）。

use async_trait::async_trait;
use erp_core::common::time::Instant;
use erp_core::ids::SalesOrderId;
use mongodb::Database;
use persistence_core::Executor;

use crate::entity::sales_order::{CollectionProgress, FulfillmentProgress, InvoiceProgress, SalesOrder};
use crate::ports::sales_order::SalesMoneyProgressPort;
use crate::repository::SalesOrderExt;
use crate::{Error, Result};

/// 按应收子账事实刷新销售单回款/开票进度与关闭状态。
///
/// 回款/开票进度从子账开放余额派生；关闭状态按 §9.3 判定：全部明细履约完成
/// 且客户应收结清后自动结案（开票完成不参与关闭判定）。任一字段变化时更新
/// 销售单并写版本触及；无变化时不写。
///
/// # 参数
/// * `db` - 数据库实例
/// * `port` - 保留逐应收子账余额的财务事实端口
/// * `session` - 事务会话执行器
/// * `sales_order_id` - 销售单
/// * `actor_id` - 审计操作人
/// * `fulfillment` - 外部已算好的履约进度；`None` 表示不修改履约进度，
///   关闭状态按销售单当前履约进度推导
///
/// # 返回
/// 无返回值。
///
/// # 错误
/// 销售单不存在、销售读取、财务事实读取或仓储写入失败时返回原错误。
pub async fn update_sales_order_money_progress(
    db: &Database,
    port: &dyn SalesMoneyProgressPort,
    session: &mut dyn persistence_core::Executor,
    sales_order_id: &SalesOrderId,
    actor_id: String,
    fulfillment: Option<FulfillmentProgress>,
) -> Result<()> {
    refresh_money_progress(
        &DatabaseProgressStore { db },
        port,
        session,
        sales_order_id,
        actor_id,
        fulfillment,
        Instant::now,
    )
    .await
}

/// 资金进度刷新所需的销售存储合同；执行器由原根事务提供。
#[async_trait]
trait SalesProgressStore: Send + Sync {
    /// 读取销售稳定单；不存在保留为 `None`，由用例产生原销售错误。
    async fn load(&self, id: &SalesOrderId, executor: &mut dyn Executor) -> Result<Option<SalesOrder>>;

    /// 写入已发生变化的销售进度；仓储错误必须按原分类传播。
    async fn update(&self, order: &mut SalesOrder, executor: &mut dyn Executor) -> Result<()>;
}

/// 将窄存储合同适配到销售拥有仓储，不创建事务或替换执行器。
struct DatabaseProgressStore<'a> {
    db: &'a Database,
}

#[async_trait]
impl SalesProgressStore for DatabaseProgressStore<'_> {
    async fn load(&self, id: &SalesOrderId, executor: &mut dyn Executor) -> Result<Option<SalesOrder>> {
        Ok(self.db.sales_orders().find_by_id(id.as_ref(), executor).await?)
    }

    async fn update(&self, order: &mut SalesOrder, executor: &mut dyn Executor) -> Result<()> {
        self.db.sales_orders().update(order, executor).await?;
        Ok(())
    }
}

/// 执行真实刷新顺序：销售存在性、逐账户财务事实、取时与实体刷新、条件写入。
///
/// `now` 必须延迟至两次读取完成后调用；任何读取失败均不得刷新或写回销售。
/// 所有存储调用复用同一个执行器，无变化时保留原实体与版本。
async fn refresh_money_progress(
    store: &dyn SalesProgressStore,
    port: &dyn SalesMoneyProgressPort,
    executor: &mut dyn Executor,
    sales_order_id: &SalesOrderId,
    actor_id: String,
    fulfillment: Option<FulfillmentProgress>,
    now: impl FnOnce() -> Instant + Send,
) -> Result<()> {
    let mut order = store
        .load(sales_order_id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("销售单不存在".to_string()))?;
    let accounts = port.receivable_balances(sales_order_id, executor).await?;
    let collection = CollectionProgress::from_receivable_balances(
        accounts.iter().map(|account| (account.open_total, account.settled_total)),
    );
    let invoice = InvoiceProgress::from_receivable_balances(
        accounts.iter().map(|account| (account.open_invoiceable_total, account.invoiced_total)),
    );
    if !order.refresh_progress(fulfillment, collection, invoice, now(), actor_id) {
        return Ok(());
    }
    store.update(&mut order, executor).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;
    use std::sync::{Arc, Mutex};

    use erp_core::ids::{CustomerAccountId, PartyId};
    use erp_core::money::Amount;
    use mongodb::ClientSession;

    use super::*;
    use crate::entity::sales_order::{
        BusinessType, CloseStatus, CommercialStatus, OriginSystem, ReviewStatus, SalesOrderData,
    };
    use crate::ports::sales_order::ReceivableBalanceFact;

    #[derive(Clone, Copy)]
    enum Failure {
        SalesRead,
        MoneyRead,
        Write,
    }

    struct State {
        events: Vec<&'static str>,
        executors: Vec<usize>,
        order: Option<SalesOrder>,
        facts: Vec<ReceivableBalanceFact>,
        failure: Option<Failure>,
        writes: Vec<SalesOrder>,
    }

    /// 同一 fixture 实现两个独立合同，记录生产 helper 的实际调用与持久化结果。
    struct Fixture(Arc<Mutex<State>>);

    impl Fixture {
        fn new(order: Option<SalesOrder>, facts: Vec<ReceivableBalanceFact>) -> Self {
            Self(Arc::new(Mutex::new(State {
                events: Vec::new(),
                executors: Vec::new(),
                order,
                facts,
                failure: None,
                writes: Vec::new(),
            })))
        }

        fn record(&self, event: &'static str, executor: &mut dyn Executor) {
            let mut state = self.0.lock().unwrap();
            state.events.push(event);
            state.executors.push(executor_address(executor));
            assert!(executor.session().is_none());
        }

        async fn run(
            &self,
            executor: &mut dyn Executor,
            fulfillment: Option<FulfillmentProgress>,
            at: Instant,
        ) -> Result<()> {
            let state = self.0.clone();
            refresh_money_progress(
                self,
                self,
                executor,
                &SalesOrderId::new("so-1"),
                "actor-1".into(),
                fulfillment,
                move || {
                    state.lock().unwrap().events.push("clock");
                    at
                },
            )
            .await
        }

        fn stored(&self) -> SalesOrder {
            self.0.lock().unwrap().order.clone().unwrap()
        }

        fn assert_events(&self, expected: &[&str]) {
            assert_eq!(self.0.lock().unwrap().events, expected);
        }
    }

    #[async_trait]
    impl SalesProgressStore for Fixture {
        async fn load(&self, id: &SalesOrderId, executor: &mut dyn Executor) -> Result<Option<SalesOrder>> {
            assert_eq!(id.as_ref(), "so-1");
            self.record("sales.read", executor);
            let state = self.0.lock().unwrap();
            if matches!(state.failure, Some(Failure::SalesRead)) {
                return Err(Error::Internal("销售读取失败".into()));
            }
            Ok(state.order.clone())
        }

        async fn update(&self, order: &mut SalesOrder, executor: &mut dyn Executor) -> Result<()> {
            self.record("sales.write", executor);
            let mut state = self.0.lock().unwrap();
            state.writes.push(order.clone());
            if matches!(state.failure, Some(Failure::Write)) {
                return Err(persistence_core::Error::OptimisticLockingError.into());
            }
            state.order = Some(order.clone());
            Ok(())
        }
    }

    #[async_trait]
    impl SalesMoneyProgressPort for Fixture {
        async fn receivable_balances(
            &self,
            id: &SalesOrderId,
            executor: &mut dyn Executor,
        ) -> Result<Vec<ReceivableBalanceFact>> {
            assert_eq!(id.as_ref(), "so-1");
            self.record("money.read", executor);
            let state = self.0.lock().unwrap();
            if matches!(state.failure, Some(Failure::MoneyRead)) {
                return Err(Error::Internal("财务事实读取失败".into()));
            }
            Ok(state.facts.clone())
        }
    }

    /// 非零大小执行器记录访问次数，不构建 MongoDB 客户端或会话。
    #[derive(Default)]
    struct RecordingExecutor {
        visits: usize,
    }

    impl Executor for RecordingExecutor {
        fn session(&mut self) -> Option<&mut ClientSession> {
            self.visits += 1;
            None
        }
    }

    fn executor_address(executor: &mut dyn Executor) -> usize {
        executor as *mut dyn Executor as *mut () as usize
    }

    fn order() -> SalesOrder {
        let mut order = SalesOrder::new(
            SalesOrderId::new("so-1"),
            SalesOrderData {
                sales_owner_user_id: "admin-1".to_string(),
                business_org_unit_id: "org-sales".to_string(),
                order_no: "SO-1".into(),
                business_type: BusinessType::GoodsService,
                origin_system: OriginSystem::Erp,
                source_identity_id: None,
                customer_id: CustomerAccountId::new("customer-1"),
                contract_id: None,
                settlement_party_id: PartyId::new("party-1"),
                source_status_code: None,
            },
            "creator-1",
        )
        .unwrap();
        order.commercial_status = CommercialStatus::Effective;
        order.stable.status = CommercialStatus::Effective;
        order.review_status = ReviewStatus::Approved;
        order
    }

    fn fact(open: &str, settled: &str, invoiceable: &str, invoiced: &str) -> ReceivableBalanceFact {
        ReceivableBalanceFact {
            open_total: Amount::from_str(open).unwrap(),
            settled_total: Amount::from_str(settled).unwrap(),
            open_invoiceable_total: Amount::from_str(invoiceable).unwrap(),
            invoiced_total: Amount::from_str(invoiced).unwrap(),
        }
    }

    #[tokio::test]
    async fn progress_reads_then_refreshes_and_writes_with_the_same_executor() {
        let fixture = Fixture::new(Some(order()), vec![fact("0", "100", "0", "100")]);
        let mut executor = RecordingExecutor::default();
        let expected_executor = executor_address(&mut executor);
        let at = Instant::from_unix_secs(600);
        fixture.run(&mut executor, Some(FulfillmentProgress::Completed), at).await.unwrap();

        fixture.assert_events(&["sales.read", "money.read", "clock", "sales.write"]);
        assert_eq!(fixture.0.lock().unwrap().executors, vec![expected_executor; 3]);
        assert_eq!(executor.visits, 3);
        let saved = fixture.stored();
        assert_eq!(saved.fulfillment_progress, FulfillmentProgress::Completed);
        assert_eq!(saved.collection_progress, CollectionProgress::Settled);
        assert_eq!(saved.invoice_progress, InvoiceProgress::Completed);
        assert_eq!(saved.close_status, CloseStatus::Closed);
        assert_eq!(saved.closed_at, Some(at));
        assert_eq!(saved.stable.updated_by, "actor-1");
    }

    #[tokio::test]
    async fn missing_sales_order_does_not_call_money_port_or_clock() {
        let fixture = Fixture::new(None, vec![fact("0", "100", "0", "100")]);
        let error = fixture
            .run(&mut RecordingExecutor::default(), None, Instant::from_unix_secs(600))
            .await
            .unwrap_err();
        assert!(matches!(&error, Error::NotFound(message) if message == "销售单不存在"));
        assert_eq!(error.to_string(), "数据不存在: 销售单不存在");
        fixture.assert_events(&["sales.read"]);
        assert!(fixture.0.lock().unwrap().writes.is_empty());
    }

    #[tokio::test]
    async fn sales_read_error_stops_before_money_read() {
        let fixture = Fixture::new(Some(order()), Vec::new());
        fixture.0.lock().unwrap().failure = Some(Failure::SalesRead);
        let error = fixture
            .run(&mut RecordingExecutor::default(), None, Instant::from_unix_secs(600))
            .await
            .unwrap_err();
        assert!(matches!(error, Error::Internal(message) if message == "销售读取失败"));
        fixture.assert_events(&["sales.read"]);
        assert!(fixture.0.lock().unwrap().writes.is_empty());
    }

    #[tokio::test]
    async fn money_read_error_propagates_without_clock_or_sales_write() {
        let original = order();
        let fixture = Fixture::new(Some(original.clone()), Vec::new());
        fixture.0.lock().unwrap().failure = Some(Failure::MoneyRead);
        let error = fixture
            .run(
                &mut RecordingExecutor::default(),
                Some(FulfillmentProgress::Completed),
                Instant::from_unix_secs(600),
            )
            .await
            .unwrap_err();
        assert!(matches!(error, Error::Internal(message) if message == "财务事实读取失败"));
        fixture.assert_events(&["sales.read", "money.read"]);
        assert!(fixture.0.lock().unwrap().writes.is_empty());
        assert_eq!(fixture.stored(), original);
    }

    #[tokio::test]
    async fn changed_progress_write_failure_remains_a_conflict() {
        let original = order();
        let fixture = Fixture::new(Some(original.clone()), vec![fact("0", "100", "0", "100")]);
        fixture.0.lock().unwrap().failure = Some(Failure::Write);
        let error = fixture
            .run(&mut RecordingExecutor::default(), None, Instant::from_unix_secs(600))
            .await
            .unwrap_err();
        assert!(matches!(error, Error::ConflictError(message)
            if message == "数据已被其他请求修改，请刷新后重试"));
        fixture.assert_events(&["sales.read", "money.read", "clock", "sales.write"]);
        let state = fixture.0.lock().unwrap();
        assert_eq!(state.writes.len(), 1);
        assert_eq!(state.writes[0].collection_progress, CollectionProgress::Settled);
        assert_eq!(state.order.as_ref(), Some(&original));
    }

    #[tokio::test]
    async fn unchanged_progress_does_not_write_or_touch_the_order() {
        let mut original = order();
        original.fulfillment_progress = FulfillmentProgress::Completed;
        original.collection_progress = CollectionProgress::Settled;
        original.invoice_progress = InvoiceProgress::Completed;
        original.close_status = CloseStatus::Closed;
        original.closed_at = Some(Instant::from_unix_secs(100));
        let fixture = Fixture::new(Some(original.clone()), vec![fact("0", "100", "0", "100")]);
        fixture.run(&mut RecordingExecutor::default(), None, Instant::from_unix_secs(600)).await.unwrap();
        fixture.assert_events(&["sales.read", "money.read", "clock"]);
        assert!(fixture.0.lock().unwrap().writes.is_empty());
        assert_eq!(fixture.stored(), original);
    }

    #[tokio::test]
    async fn balance_matrix_preserves_empty_zero_partial_complete_and_independent_progress() {
        let cases = [
            (Vec::new(), CollectionProgress::NotCollected, InvoiceProgress::NotInvoiced),
            (vec![fact("0", "0", "0", "0")], CollectionProgress::NotCollected, InvoiceProgress::NotInvoiced),
            (
                vec![fact("100", "0", "100", "0")],
                CollectionProgress::NotCollected,
                InvoiceProgress::NotInvoiced,
            ),
            (
                vec![fact("50", "50", "50", "50")],
                CollectionProgress::PartiallyCollected,
                InvoiceProgress::PartiallyInvoiced,
            ),
            (
                vec![fact("0", "100", "0", "100"), fact("0", "30", "0", "30")],
                CollectionProgress::Settled,
                InvoiceProgress::Completed,
            ),
            (
                vec![fact("0", "100", "50", "50")],
                CollectionProgress::Settled,
                InvoiceProgress::PartiallyInvoiced,
            ),
            (
                vec![fact("50", "50", "0", "100")],
                CollectionProgress::PartiallyCollected,
                InvoiceProgress::Completed,
            ),
        ];
        for (facts, collection, invoice) in cases {
            let fixture = Fixture::new(Some(order()), facts);
            fixture
                .run(
                    &mut RecordingExecutor::default(),
                    Some(FulfillmentProgress::PartiallyFulfilled),
                    Instant::from_unix_secs(600),
                )
                .await
                .unwrap();
            let saved = fixture.stored();
            assert_eq!(saved.collection_progress, collection);
            assert_eq!(saved.invoice_progress, invoice);
            assert_eq!(saved.fulfillment_progress, FulfillmentProgress::PartiallyFulfilled);
            assert_ne!(saved.close_status, CloseStatus::Closed);
        }
    }

    #[tokio::test]
    async fn settled_account_plus_zero_account_remains_partial_and_cannot_close() {
        let fixture =
            Fixture::new(Some(order()), vec![fact("0", "100", "0", "100"), fact("0", "0", "0", "0")]);
        fixture
            .run(
                &mut RecordingExecutor::default(),
                Some(FulfillmentProgress::Completed),
                Instant::from_unix_secs(600),
            )
            .await
            .unwrap();
        let saved = fixture.stored();
        assert_eq!(saved.collection_progress, CollectionProgress::PartiallyCollected);
        assert_eq!(saved.invoice_progress, InvoiceProgress::PartiallyInvoiced);
        assert_eq!(saved.close_status, CloseStatus::Closeable);
        assert!(saved.closed_at.is_none());
    }

    #[tokio::test]
    async fn absent_fulfillment_keeps_current_value_and_uninvoiced_accounts_do_not_block_closing() {
        let mut original = order();
        original.fulfillment_progress = FulfillmentProgress::Completed;
        let fixture = Fixture::new(Some(original), vec![fact("0", "100", "100", "0")]);
        let at = Instant::from_unix_secs(600);
        fixture.run(&mut RecordingExecutor::default(), None, at).await.unwrap();
        let saved = fixture.stored();
        assert_eq!(saved.fulfillment_progress, FulfillmentProgress::Completed);
        assert_eq!(saved.invoice_progress, InvoiceProgress::NotInvoiced);
        assert_eq!(saved.close_status, CloseStatus::Closed);
        assert_eq!(saved.closed_at, Some(at));
    }

    #[tokio::test]
    async fn later_invoice_updates_preserve_the_first_closed_at() {
        let mut original = order();
        original.fulfillment_progress = FulfillmentProgress::Completed;
        let fixture = Fixture::new(Some(original), vec![fact("0", "100", "100", "0")]);
        let first = Instant::from_unix_secs(600);
        fixture.run(&mut RecordingExecutor::default(), None, first).await.unwrap();
        fixture.0.lock().unwrap().facts = vec![fact("0", "100", "0", "100")];
        fixture.run(&mut RecordingExecutor::default(), None, Instant::from_unix_secs(900)).await.unwrap();
        let saved = fixture.stored();
        assert_eq!(saved.invoice_progress, InvoiceProgress::Completed);
        assert_eq!(saved.closed_at, Some(first));
        assert_eq!(fixture.0.lock().unwrap().writes.len(), 2);
    }
}
