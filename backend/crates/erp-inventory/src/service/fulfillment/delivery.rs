//! 仓发逐行消耗预占、释放后扣减余额并追加库存事实。

use erp_core::common::source::SourceType;
use erp_core::common::time::Instant;
use erp_core::ids::{
    SalesOrderLineId, StockMovementId, StockReservationEntryId, StockReservationId, WarehouseId,
};
use erp_core::money::Quantity;
use id_generator::next_id;
use mongodb::Database;
use persistence_core::Executor;

use super::{InventoryWriteStore, MongoInventoryStore};
use crate::{
    Error, MovementDirection, MovementType, ReservationEntryType, Result, StockMovement, StockMovementData,
    StockReservationEntry, StockReservationEntryData,
};

/// 库存消费的单条仓发事实；可选来源必须在原逐行校验位置检查。
pub struct WarehouseShipmentLine {
    /// 发货单身份，供预占流水和出库事实关联。
    pub delivery_id: String,
    /// 当前发货行身份。
    pub line_id: String,
    /// 冻结销售明细身份。
    pub sales_order_line_id: SalesOrderLineId,
    /// 原发货行预占引用；未提供时保留原业务错误。
    pub stock_reservation_id: Option<StockReservationId>,
    /// 发货表头仓库；在预占归属和数量校验后检查。
    pub warehouse_id: Option<WarehouseId>,
    /// 本行发货数量。
    pub quantity: Quantity,
}

/// 过账单条仓发行（预占消耗 + 出库流水 + 余额，位于调用方事务内）。
///
/// # 参数
/// * `db` - 库存数据库实例
/// * `executor` - 调用方事务执行器，不得另开事务
/// * `input` - 当前发货行的最小库存消费事实
/// * `occurred_at` - 原发货流程在读取全部发货行后取得的业务时间
/// * `actor_id` - 已认证操作人的记录身份
///
/// # 返回
/// 无返回值；写入失败时返回错误。
///
/// # 错误
/// 预占不存在/归属不符/数量不足、余额缺失或写入失败时返回原错误。
///
/// # 关键业务约束
/// 调用方逐行调用，不预校验后续行；释放预占余额必须先于扣减可用量。
pub async fn post_warehouse_ship_line(
    db: &Database,
    executor: &mut dyn Executor,
    input: &WarehouseShipmentLine,
    occurred_at: Instant,
    actor_id: &str,
) -> Result<()> {
    post_with_store(&mut MongoInventoryStore(db), executor, input, occurred_at, actor_id).await
}

/// 实际逐行过账算法；库存仓储和失败注入替身均从此入口执行。
async fn post_with_store(
    store: &mut impl InventoryWriteStore,
    executor: &mut dyn Executor,
    input: &WarehouseShipmentLine,
    occurred_at: Instant,
    actor_id: &str,
) -> Result<()> {
    let reservation_id = input
        .stock_reservation_id
        .clone()
        .ok_or_else(|| Error::BusinessLogicError("仓发必须消耗有效预占".to_string()))?;
    let reservation = store
        .find_reservation(reservation_id.as_ref(), executor)
        .await?
        .ok_or_else(|| Error::BusinessLogicError("库存预占不存在".to_string()))?;
    if reservation.sales_order_line_id != input.sales_order_line_id {
        return Err(Error::BusinessLogicError("库存预占不属于本销售明细，不能消耗".to_string()));
    }
    if reservation.reserved_quantity.to_decimal() < input.quantity.to_decimal() {
        return Err(Error::BusinessLogicError("为这单留的货不足，无法发货".to_string()));
    }
    let warehouse_id =
        input.warehouse_id.clone().ok_or_else(|| Error::BusinessLogicError("仓发缺少发货仓".to_string()))?;
    if reservation.warehouse_id != warehouse_id {
        return Err(Error::BusinessLogicError("库存预占不属于本发货仓，无法发货".to_string()));
    }
    let balance = store
        .balance(&warehouse_id, &reservation.sku_id, executor)
        .await?
        .ok_or_else(|| Error::BusinessLogicError("库存余额不存在，无法发货".to_string()))?;
    if !store.consume_reservation(&reservation.id, input.quantity, executor).await? {
        return Err(Error::BusinessLogicError("预占数量不足或状态不符，无法消耗".to_string()));
    }
    let entry = StockReservationEntry::new(
        StockReservationEntryId::new(next_id()),
        StockReservationEntryData {
            reservation_id: reservation.id.clone().into(),
            entry_type: ReservationEntryType::Consume,
            quantity: input.quantity,
            source_document_id: input.delivery_id.clone(),
        },
    )?;
    store.create_entry(&entry, executor).await?;
    // 先释放预占再扣可用：预占建立时已扣减 available（reserved += q / available -= q），
    // 消耗本单预占发货时 available 已不含这部分，必须先释放（available += q）才能扣减
    if !store.release_reserved(&balance, input.quantity, executor).await? {
        return Err(Error::BusinessLogicError("预占余额不足，无法发货".to_string()));
    }
    if !store.deduct_available(&balance, input.quantity, executor).await? {
        return Err(Error::BusinessLogicError("可用库存不足，无法发货".to_string()));
    }
    let movement = StockMovement::new(
        StockMovementId::new(next_id()),
        StockMovementData {
            warehouse_id,
            sku_id: reservation.sku_id,
            movement_type: MovementType::WarehouseShipOut,
            direction: MovementDirection::Decrease,
            quantity: input.quantity,
            source_document_id: input.delivery_id.clone(),
            source_line_id: Some(input.line_id.clone()),
            reversal_of_movement_id: None,
            fact_no: next_id(),
            occurred_at,
            recorded_at: occurred_at,
            recorded_by: actor_id.to_string(),
            source_type: SourceType::Erp,
            source_reference: None,
            reason_code: None,
            reason_text: None,
        },
    )?;
    store.movement(&movement, executor).await?;
    // 余额记录最后流水（台账「最后变动」列），与数量增减同事务
    if !store.last_movement(&balance, &movement.base.id, executor).await? {
        return Err(Error::BusinessLogicError("库存余额行不存在".to_string()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use async_trait::async_trait;
    use erp_core::ids::SkuId;

    use super::super::ReservationFact;
    use super::*;

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum Call {
        Reservation,
        Balance,
        Consume,
        Entry,
        Release,
        Deduct,
        Movement,
        LastMovement,
    }

    /// 非零大小执行器，避免零大小地址相等掩盖替换实例。
    struct TestExecutor {
        _identity: u8,
    }
    impl Executor for TestExecutor {
        fn session(&mut self) -> Option<&mut mongodb::ClientSession> {
            None
        }
    }

    struct RecordingStore {
        executor: usize,
        calls: Vec<Call>,
        fail_at: Option<Call>,
        false_at: Option<Call>,
        reservation: Option<ReservationFact>,
        balance: Option<String>,
        entry: Option<StockReservationEntry>,
        movement: Option<StockMovement>,
        available: i64,
        reserved: i64,
        on_hand: i64,
    }
    impl RecordingStore {
        fn new(executor: &mut TestExecutor) -> Self {
            Self {
                executor: executor as *mut TestExecutor as usize,
                calls: vec![],
                fail_at: None,
                false_at: None,
                reservation: Some(ReservationFact {
                    id: "reservation-1".into(),
                    sales_order_line_id: SalesOrderLineId::new("sales-line-1"),
                    reserved_quantity: quantity("2"),
                    warehouse_id: WarehouseId::new("warehouse-1"),
                    sku_id: SkuId::new("sku-1"),
                }),
                balance: Some("balance-1".into()),
                entry: None,
                movement: None,
                available: 0,
                reserved: 2,
                on_hand: 2,
            }
        }
        fn visit(&mut self, call: Call, executor: &mut dyn Executor) -> Result<()> {
            assert_eq!(executor as *mut dyn Executor as *mut () as usize, self.executor);
            self.calls.push(call);
            if self.fail_at == Some(call) {
                return Err(Error::ConflictError("原库存写入冲突".into()));
            }
            Ok(())
        }
    }

    #[async_trait]
    impl InventoryWriteStore for RecordingStore {
        async fn find_reservation(
            &mut self,
            id: &str,
            executor: &mut dyn Executor,
        ) -> Result<Option<ReservationFact>> {
            self.visit(Call::Reservation, executor)?;
            assert_eq!(id, "reservation-1");
            Ok(self.reservation.clone())
        }
        async fn balance(
            &mut self,
            warehouse: &WarehouseId,
            sku: &SkuId,
            executor: &mut dyn Executor,
        ) -> Result<Option<String>> {
            self.visit(Call::Balance, executor)?;
            assert_eq!(warehouse.as_ref(), "warehouse-1");
            assert_eq!(sku.as_ref(), "sku-1");
            Ok(self.balance.clone())
        }
        async fn consume_reservation(
            &mut self,
            id: &str,
            amount: Quantity,
            executor: &mut dyn Executor,
        ) -> Result<bool> {
            self.visit(Call::Consume, executor)?;
            assert_eq!(id, "reservation-1");
            assert_eq!(amount, quantity("2"));
            Ok(self.false_at != Some(Call::Consume))
        }
        async fn create_entry(
            &mut self,
            entry: &StockReservationEntry,
            executor: &mut dyn Executor,
        ) -> Result<()> {
            self.visit(Call::Entry, executor)?;
            self.entry = Some(entry.clone());
            Ok(())
        }
        async fn increase(
            &mut self,
            _id: &str,
            _amount: Quantity,
            executor: &mut dyn Executor,
        ) -> Result<bool> {
            self.visit(Call::Balance, executor)?;
            Ok(false)
        }
        async fn create_balance(
            &mut self,
            _balance: &crate::StockBalance,
            executor: &mut dyn Executor,
        ) -> Result<()> {
            self.visit(Call::Balance, executor)?;
            Ok(())
        }
        async fn create_reservation(
            &mut self,
            _reservation: &crate::StockReservation,
            executor: &mut dyn Executor,
        ) -> Result<()> {
            self.visit(Call::Reservation, executor)?;
            Ok(())
        }
        async fn reserve(
            &mut self,
            _id: &str,
            _amount: Quantity,
            executor: &mut dyn Executor,
        ) -> Result<bool> {
            self.visit(Call::Balance, executor)?;
            Ok(false)
        }
        async fn release_reserved(
            &mut self,
            id: &str,
            amount: Quantity,
            executor: &mut dyn Executor,
        ) -> Result<bool> {
            self.visit(Call::Release, executor)?;
            assert_eq!(id, "balance-1");
            assert_eq!(amount, quantity("2"));
            if self.false_at == Some(Call::Release) || self.reserved < 2 {
                return Ok(false);
            }
            self.reserved -= 2;
            self.available += 2;
            Ok(true)
        }
        async fn deduct_available(
            &mut self,
            id: &str,
            amount: Quantity,
            executor: &mut dyn Executor,
        ) -> Result<bool> {
            self.visit(Call::Deduct, executor)?;
            assert_eq!(id, "balance-1");
            assert_eq!(amount, quantity("2"));
            if self.false_at == Some(Call::Deduct) || self.available < 2 {
                return Ok(false);
            }
            self.available -= 2;
            self.on_hand -= 2;
            Ok(true)
        }
        async fn movement(&mut self, movement: &StockMovement, executor: &mut dyn Executor) -> Result<()> {
            self.visit(Call::Movement, executor)?;
            self.movement = Some(movement.clone());
            Ok(())
        }
        async fn last_movement(
            &mut self,
            balance_id: &str,
            movement_id: &str,
            executor: &mut dyn Executor,
        ) -> Result<bool> {
            self.visit(Call::LastMovement, executor)?;
            assert_eq!(balance_id, "balance-1");
            assert_eq!(movement_id, self.movement.as_ref().expect("流水已写入").base.id);
            Ok(self.false_at != Some(Call::LastMovement))
        }
    }

    fn quantity(value: &str) -> Quantity {
        Quantity::from_str(value).unwrap()
    }
    fn input() -> WarehouseShipmentLine {
        WarehouseShipmentLine {
            delivery_id: "delivery-1".into(),
            line_id: "delivery-line-1".into(),
            sales_order_line_id: SalesOrderLineId::new("sales-line-1"),
            stock_reservation_id: Some(StockReservationId::new("reservation-1")),
            warehouse_id: Some(WarehouseId::new("warehouse-1")),
            quantity: quantity("2"),
        }
    }
    fn sequence() -> [Call; 8] {
        [
            Call::Reservation,
            Call::Balance,
            Call::Consume,
            Call::Entry,
            Call::Release,
            Call::Deduct,
            Call::Movement,
            Call::LastMovement,
        ]
    }

    /// 从可用量为零的已预占余额出发，生产算法必须先释放再扣减，并保留源引用、时间和操作人。
    #[tokio::test]
    async fn shipment_releases_reserved_before_deducting_and_keeps_frozen_facts() {
        let mut executor = TestExecutor { _identity: 1 };
        let mut store = RecordingStore::new(&mut executor);
        let occurred_at = Instant::from_unix_secs(12345);
        post_with_store(&mut store, &mut executor, &input(), occurred_at, "actor-1").await.unwrap();
        assert_eq!(store.calls, sequence());
        assert_eq!((store.available, store.reserved, store.on_hand), (0, 0, 0));
        let entry = store.entry.unwrap();
        assert_eq!(entry.reservation_id.as_ref(), "reservation-1");
        assert_eq!(entry.source_document_id, "delivery-1");
        assert_eq!(entry.entry_type, ReservationEntryType::Consume);
        assert_eq!(entry.quantity, quantity("2"));
        let movement = store.movement.unwrap();
        assert_eq!(movement.source_document_id, "delivery-1");
        assert_eq!(movement.source_line_id.as_deref(), Some("delivery-line-1"));
        assert_eq!(movement.warehouse_id.as_ref(), "warehouse-1");
        assert_eq!(movement.sku_id.as_ref(), "sku-1");
        assert_eq!(movement.movement_type, MovementType::WarehouseShipOut);
        assert_eq!(movement.direction, MovementDirection::Decrease);
        assert_eq!(movement.quantity, quantity("2"));
        assert_eq!(movement.fact.occurred_at, occurred_at);
        assert_eq!(movement.fact.recorded_at, occurred_at);
        assert_eq!(movement.fact.recorded_by, "actor-1");
        assert_eq!(movement.fact.source_type, SourceType::Erp);
        assert!(movement.reversal_of_movement_id.is_none());
        assert!(!movement.base.id.is_empty());
        assert!(!movement.fact.fact_no.is_empty());
        assert_ne!(movement.base.id, entry.base.id);
    }

    /// 实际仓储入口的每个读取或写入失败均保留原错，停止后续读写。
    #[tokio::test]
    async fn shipment_stops_after_each_repository_failure_with_same_executor() {
        let expected = sequence();
        for (index, call) in expected.into_iter().enumerate() {
            let mut executor = TestExecutor { _identity: 1 };
            let mut store = RecordingStore::new(&mut executor);
            store.fail_at = Some(call);
            let error =
                post_with_store(&mut store, &mut executor, &input(), Instant::from_unix_secs(10), "actor-1")
                    .await
                    .unwrap_err();
            assert!(matches!(error, Error::ConflictError(message) if message == "原库存写入冲突"));
            assert_eq!(store.calls, expected[..=index]);
        }
    }

    /// 四个条件写入返回 false 时保持原业务错误，不合并释放与扣减步骤。
    #[tokio::test]
    async fn shipment_keeps_conditional_write_failure_messages_and_stops() {
        for (call, message) in [
            (Call::Consume, "预占数量不足或状态不符，无法消耗"),
            (Call::Release, "预占余额不足，无法发货"),
            (Call::Deduct, "可用库存不足，无法发货"),
            (Call::LastMovement, "库存余额行不存在"),
        ] {
            let mut executor = TestExecutor { _identity: 1 };
            let mut store = RecordingStore::new(&mut executor);
            store.false_at = Some(call);
            let error =
                post_with_store(&mut store, &mut executor, &input(), Instant::from_unix_secs(10), "actor-1")
                    .await
                    .unwrap_err();
            assert!(matches!(error, Error::BusinessLogicError(actual) if actual == message));
            let expected = sequence();
            let index = expected.iter().position(|step| *step == call).unwrap();
            assert_eq!(store.calls, expected[..=index]);
        }
    }

    /// 缺少预占引用优先于仓库与其他行事实，不触发任何仓储读取。
    #[tokio::test]
    async fn shipment_requires_reservation_before_any_read() {
        let mut executor = TestExecutor { _identity: 1 };
        let mut store = RecordingStore::new(&mut executor);
        let mut line = input();
        line.stock_reservation_id = None;
        line.warehouse_id = None;
        let error = post_with_store(&mut store, &mut executor, &line, Instant::from_unix_secs(10), "actor-1")
            .await
            .unwrap_err();
        assert!(matches!(error, Error::BusinessLogicError(message) if message == "仓发必须消耗有效预占"));
        assert!(store.calls.is_empty());
    }

    /// 销售归属、预占数量、仓库存在与仓库归属按原顺序失败，之后才允许读取余额。
    #[tokio::test]
    async fn shipment_preserves_reservation_quantity_and_warehouse_first_error_order() {
        for (case, message) in [
            (0, "库存预占不属于本销售明细，不能消耗"),
            (1, "为这单留的货不足，无法发货"),
            (2, "仓发缺少发货仓"),
            (3, "库存预占不属于本发货仓，无法发货"),
        ] {
            let mut executor = TestExecutor { _identity: 1 };
            let mut store = RecordingStore::new(&mut executor);
            let mut line = input();
            let reservation = store.reservation.as_mut().unwrap();
            if case == 0 {
                reservation.sales_order_line_id = SalesOrderLineId::new("other-line");
            }
            if case <= 1 {
                reservation.reserved_quantity = quantity("1");
            }
            if case <= 2 {
                line.warehouse_id = None;
            } else {
                line.warehouse_id = Some(WarehouseId::new("other-warehouse"));
            }
            let error =
                post_with_store(&mut store, &mut executor, &line, Instant::from_unix_secs(10), "actor-1")
                    .await
                    .unwrap_err();
            assert!(matches!(error, Error::BusinessLogicError(actual) if actual == message));
            assert_eq!(store.calls, [Call::Reservation]);
        }
    }

    /// 缺失预占与余额使用原业务错误，缺失后不进入消耗。
    #[tokio::test]
    async fn shipment_preserves_missing_stock_fact_errors() {
        for missing_reservation in [true, false] {
            let mut executor = TestExecutor { _identity: 1 };
            let mut store = RecordingStore::new(&mut executor);
            if missing_reservation {
                store.reservation = None;
            } else {
                store.balance = None;
            }
            let error =
                post_with_store(&mut store, &mut executor, &input(), Instant::from_unix_secs(10), "actor-1")
                    .await
                    .unwrap_err();
            let expected = if missing_reservation {
                "库存预占不存在"
            } else {
                "库存余额不存在，无法发货"
            };
            assert!(matches!(error, Error::BusinessLogicError(message) if message == expected));
            assert_eq!(
                store.calls,
                if missing_reservation {
                    vec![Call::Reservation]
                } else {
                    vec![Call::Reservation, Call::Balance]
                }
            );
        }
    }
}
