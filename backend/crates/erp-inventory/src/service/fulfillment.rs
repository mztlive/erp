//! 履约入库的库存余额、流水与销售预占写入；复用调用方唯一执行器。
pub mod delivery;

use async_trait::async_trait;
use erp_core::common::source::SourceType;
use erp_core::common::time::Instant;
use erp_core::ids::{
    PurchaseLineSalesAllocationId, PurchaseReceiptLineId, SalesOrderLineId, SkuId, StockBalanceId,
    StockMovementId, StockReservationEntryId, StockReservationId, WarehouseId,
};
use erp_core::money::Quantity;
use id_generator::next_id;
use mongodb::Database;
use persistence_core::Executor;

use crate::entity::inventory::zero_quantity;
use crate::repository::prelude::*;
use crate::{
    Error, InventoryExt, MovementDirection, MovementType, ReservationEntryType, ReservationStatus, Result,
    StockBalance, StockBalanceData, StockMovement, StockMovementData, StockReservation, StockReservationData,
    StockReservationEntry, StockReservationEntryData, StockReservationSourceType,
};
/// 收货与仓发共用的库存写入仓储边界；事实构造与数量规则由库存服务唯一持有。
#[async_trait]
pub(crate) trait InventoryWriteStore: Send {
    async fn balance(
        &mut self,
        warehouse: &WarehouseId,
        sku: &SkuId,
        ex: &mut dyn Executor,
    ) -> Result<Option<String>>;
    async fn increase(&mut self, id: &str, quantity: Quantity, ex: &mut dyn Executor) -> Result<bool>;
    async fn create_balance(&mut self, balance: &StockBalance, ex: &mut dyn Executor) -> Result<()>;
    async fn movement(&mut self, movement: &StockMovement, ex: &mut dyn Executor) -> Result<()>;
    async fn last_movement(
        &mut self,
        balance_id: &str,
        movement_id: &str,
        ex: &mut dyn Executor,
    ) -> Result<bool>;
    async fn create_reservation(
        &mut self,
        reservation: &StockReservation,
        ex: &mut dyn Executor,
    ) -> Result<()>;
    async fn reserve(&mut self, id: &str, quantity: Quantity, ex: &mut dyn Executor) -> Result<bool>;
    async fn create_entry(&mut self, entry: &StockReservationEntry, ex: &mut dyn Executor) -> Result<()>;
    async fn find_reservation(&mut self, id: &str, ex: &mut dyn Executor) -> Result<Option<ReservationFact>>;
    async fn consume_reservation(
        &mut self,
        id: &str,
        quantity: Quantity,
        ex: &mut dyn Executor,
    ) -> Result<bool>;
    async fn release_reserved(&mut self, id: &str, quantity: Quantity, ex: &mut dyn Executor)
    -> Result<bool>;
    async fn deduct_available(&mut self, id: &str, quantity: Quantity, ex: &mut dyn Executor)
    -> Result<bool>;
}

/// 当前实际消费的预占事实，查询顺序由生产过账函数控制。
#[derive(Clone)]
pub(crate) struct ReservationFact {
    /// 预占主键。
    pub(crate) id: String,
    /// 归属稳定销售行。
    pub(crate) sales_order_line_id: SalesOrderLineId,
    /// 当前有效预占。
    pub(crate) reserved_quantity: Quantity,
    /// 预占仓库。
    pub(crate) warehouse_id: WarehouseId,
    /// 预占 SKU。
    pub(crate) sku_id: SkuId,
}

pub(crate) struct MongoInventoryStore<'a>(pub(crate) &'a Database);

#[async_trait]
impl InventoryWriteStore for MongoInventoryStore<'_> {
    async fn balance(
        &mut self,
        warehouse: &WarehouseId,
        sku: &SkuId,
        ex: &mut dyn Executor,
    ) -> Result<Option<String>> {
        Ok(self
            .0
            .stock_balances()
            .find_by_dimensions(warehouse, sku, ex)
            .await?
            .map(|balance| balance.base.id))
    }
    async fn increase(&mut self, id: &str, quantity: Quantity, ex: &mut dyn Executor) -> Result<bool> {
        Ok(self.0.stock_balances().increase_on_hand(id, quantity, ex).await?)
    }
    async fn create_balance(&mut self, balance: &StockBalance, ex: &mut dyn Executor) -> Result<()> {
        self.0.stock_balances().create(balance, ex).await?;
        Ok(())
    }
    async fn movement(&mut self, movement: &StockMovement, ex: &mut dyn Executor) -> Result<()> {
        self.0.stock_movements().create(movement, ex).await?;
        Ok(())
    }
    async fn last_movement(
        &mut self,
        balance_id: &str,
        movement_id: &str,
        ex: &mut dyn Executor,
    ) -> Result<bool> {
        Ok(self.0.stock_balances().apply_last_movement(balance_id, movement_id, ex).await?)
    }
    async fn create_reservation(
        &mut self,
        reservation: &StockReservation,
        ex: &mut dyn Executor,
    ) -> Result<()> {
        self.0.stock_reservations().create(reservation, ex).await?;
        Ok(())
    }
    async fn reserve(&mut self, id: &str, quantity: Quantity, ex: &mut dyn Executor) -> Result<bool> {
        Ok(self.0.stock_balances().reserve_quantity(id, quantity, ex).await?)
    }
    async fn create_entry(&mut self, entry: &StockReservationEntry, ex: &mut dyn Executor) -> Result<()> {
        self.0.stock_reservation_entries().create(entry, ex).await?;
        Ok(())
    }
    async fn find_reservation(&mut self, id: &str, ex: &mut dyn Executor) -> Result<Option<ReservationFact>> {
        Ok(self.0.stock_reservations().find_by_id(id, ex).await?.map(|reservation| ReservationFact {
            id: reservation.base.id,
            sales_order_line_id: reservation.sales_order_line_id,
            reserved_quantity: reservation.reserved_quantity,
            warehouse_id: reservation.warehouse_id,
            sku_id: reservation.sku_id,
        }))
    }
    async fn consume_reservation(
        &mut self,
        id: &str,
        quantity: Quantity,
        ex: &mut dyn Executor,
    ) -> Result<bool> {
        Ok(self.0.stock_reservations().consume_quantity(id, quantity, ex).await?)
    }
    async fn release_reserved(
        &mut self,
        id: &str,
        quantity: Quantity,
        ex: &mut dyn Executor,
    ) -> Result<bool> {
        Ok(self.0.stock_balances().release_reserved(id, quantity, ex).await?)
    }
    async fn deduct_available(
        &mut self,
        id: &str,
        quantity: Quantity,
        ex: &mut dyn Executor,
    ) -> Result<bool> {
        Ok(self.0.stock_balances().deduct_available(id, quantity, ex).await?)
    }
}
/// 入库库存增加所需的消费事实；不携带履约或采购实体。
pub struct ReceiptStockFact<'a> {
    /// 入库仓。
    pub warehouse_id: &'a WarehouseId,
    /// 实物 SKU。
    pub sku_id: &'a SkuId,
    /// 本行合格数量。
    pub quantity: Quantity,
    /// 入库单身份。
    pub receipt_id: &'a str,
    /// 入库行身份。
    pub receipt_line_id: &'a str,
}
#[async_trait]
trait ReceiptStockSteps: Send {
    async fn balance(&mut self, executor: &mut dyn Executor) -> Result<String>;
    async fn movement(&mut self, executor: &mut dyn Executor) -> Result<String>;
    async fn last_movement(
        &mut self,
        balance_id: &str,
        movement_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<()>;
}
async fn execute_receipt_stock(
    steps: &mut impl ReceiptStockSteps,
    executor: &mut dyn Executor,
) -> Result<String> {
    let balance_id = steps.balance(executor).await?;
    let movement_id = steps.movement(executor).await?;
    steps.last_movement(&balance_id, &movement_id, executor).await?;
    Ok(balance_id)
}
struct ReceiptStockPosting<'a, S> {
    store: &'a mut S,
    fact: ReceiptStockFact<'a>,
    occurred_at: Instant,
    actor_id: &'a str,
}
#[async_trait]
impl<S: InventoryWriteStore> ReceiptStockSteps for ReceiptStockPosting<'_, S> {
    async fn balance(&mut self, executor: &mut dyn Executor) -> Result<String> {
        ensure_or_create_balance(
            self.store,
            executor,
            self.fact.warehouse_id,
            self.fact.sku_id,
            self.fact.quantity,
        )
        .await
    }
    async fn movement(&mut self, session: &mut dyn Executor) -> Result<String> {
        let movement = StockMovement::new(
            StockMovementId::new(next_id()),
            StockMovementData {
                warehouse_id: self.fact.warehouse_id.clone(),
                sku_id: self.fact.sku_id.clone(),
                movement_type: MovementType::PurchaseReceiptIn,
                direction: MovementDirection::Increase,
                quantity: self.fact.quantity,
                source_document_id: self.fact.receipt_id.to_string(),
                source_line_id: Some(self.fact.receipt_line_id.to_string()),
                reversal_of_movement_id: None,
                fact_no: next_id(),
                occurred_at: self.occurred_at,
                recorded_at: self.occurred_at,
                recorded_by: self.actor_id.to_string(),
                source_type: SourceType::Erp,
                source_reference: None,
                reason_code: None,
                reason_text: None,
            },
        )?;
        self.store.movement(&movement, session).await?;
        Ok(movement.base.id)
    }
    async fn last_movement(
        &mut self,
        balance_id: &str,
        movement_id: &str,
        session: &mut dyn Executor,
    ) -> Result<()> {
        // 余额记录最后流水（台账「最后变动」列），与数量增减同事务
        if !self.store.last_movement(balance_id, movement_id, session).await? {
            return Err(Error::BusinessLogicError("库存余额行不存在".to_string()));
        }
        Ok(())
    }
}
/// 在调用方事务内按余额、库存流水、最后流水顺序入账；失败立即返回根事务。
pub async fn post_receipt_stock(
    db: &Database,
    executor: &mut dyn Executor,
    fact: ReceiptStockFact<'_>,
    occurred_at: Instant,
    actor_id: &str,
) -> Result<String> {
    execute_receipt_stock(
        &mut ReceiptStockPosting { store: &mut MongoInventoryStore(db), fact, occurred_at, actor_id },
        executor,
    )
    .await
}
/// 建立/更新库存余额并返回余额主键（位于调用方事务内）。
///
/// # 参数
/// * `store` - 库存写入仓储边界
/// * `session` - 事务会话执行器
/// * `warehouse_id` - 仓库
/// * `sku_id` - SKU
/// * `quantity` - 本次入库数量
///
/// # 返回
/// 返回余额主键。
///
/// # 错误
/// 余额写入失败时返回错误。
async fn ensure_or_create_balance(
    store: &mut impl InventoryWriteStore,
    session: &mut dyn Executor,
    warehouse_id: &erp_core::ids::WarehouseId,
    sku_id: &erp_core::ids::SkuId,
    quantity: erp_core::money::Quantity,
) -> Result<String> {
    if let Some(balance_id) = store.balance(warehouse_id, sku_id, session).await? {
        if !store.increase(&balance_id, quantity, session).await? {
            return Err(Error::BusinessLogicError("库存余额行不存在".to_string()));
        }
        return Ok(balance_id);
    }
    let balance = StockBalance::new(
        StockBalanceId::new(next_id()),
        StockBalanceData {
            warehouse_id: warehouse_id.clone(),
            sku_id: sku_id.clone(),
            on_hand_quantity: quantity,
            reserved_quantity: zero_quantity(),
            available_quantity: quantity,
            last_movement_id: None,
        },
    )?;
    store.create_balance(&balance, session).await?;
    Ok(balance.base.id)
}
/// 采购收货单行的一条分配对应的库存预占消费事实。
pub struct ReceiptReservationFact<'a> {
    /// 入库仓。
    pub warehouse_id: &'a WarehouseId,
    /// 实物 SKU。
    pub sku_id: &'a SkuId,
    /// 归属稳定销售行。
    pub sales_order_line_id: SalesOrderLineId,
    /// 采购销售分配身份。
    pub allocation_id: PurchaseLineSalesAllocationId,
    /// 入库行身份。
    pub receipt_line_id: PurchaseReceiptLineId,
    /// 入库单身份。
    pub receipt_id: &'a str,
    /// 当前余额身份。
    pub balance_id: &'a str,
    /// 本分配预占数量。
    pub quantity: Quantity,
}
#[async_trait]
trait ReceiptReservationSteps: Send {
    async fn reservation(&mut self, executor: &mut dyn Executor) -> Result<String>;
    async fn reserve(&mut self, executor: &mut dyn Executor) -> Result<()>;
    async fn entry(&mut self, reservation_id: &str, executor: &mut dyn Executor) -> Result<()>;
}
async fn execute_receipt_reservation(
    steps: &mut impl ReceiptReservationSteps,
    executor: &mut dyn Executor,
) -> Result<()> {
    let reservation_id = steps.reservation(executor).await?;
    steps.reserve(executor).await?;
    steps.entry(&reservation_id, executor).await
}
struct ReceiptReservationPosting<'a, S> {
    store: &'a mut S,
    fact: ReceiptReservationFact<'a>,
}
#[async_trait]
impl<S: InventoryWriteStore> ReceiptReservationSteps for ReceiptReservationPosting<'_, S> {
    async fn reservation(&mut self, session: &mut dyn Executor) -> Result<String> {
        let reservation = StockReservation::new(
            StockReservationId::new(next_id()),
            StockReservationData {
                warehouse_id: self.fact.warehouse_id.clone(),
                sku_id: self.fact.sku_id.clone(),
                sales_order_line_id: self.fact.sales_order_line_id.clone(),
                source_type: StockReservationSourceType::PurchaseReceipt,
                purchase_line_sales_allocation_id: Some(self.fact.allocation_id.clone()),
                source_receipt_line_id: Some(self.fact.receipt_line_id.clone()),
                source_allocation_id: None,
                reserved_quantity: self.fact.quantity,
                consumed_quantity: zero_quantity(),
                released_quantity: zero_quantity(),
                status: ReservationStatus::Active,
            },
        )?;
        self.store.create_reservation(&reservation, session).await?;
        Ok(reservation.base.id)
    }
    async fn reserve(&mut self, session: &mut dyn Executor) -> Result<()> {
        if !self.store.reserve(self.fact.balance_id, self.fact.quantity, session).await? {
            return Err(Error::BusinessLogicError("可用库存不足，无法建立销售预占".to_string()));
        }
        Ok(())
    }
    async fn entry(&mut self, reservation_id: &str, session: &mut dyn Executor) -> Result<()> {
        let entry = StockReservationEntry::new(
            StockReservationEntryId::new(next_id()),
            StockReservationEntryData {
                reservation_id: reservation_id.to_string().into(),
                entry_type: ReservationEntryType::Establish,
                quantity: self.fact.quantity,
                source_document_id: self.fact.receipt_id.to_string(),
            },
        )?;
        self.store.create_entry(&entry, session).await?;
        Ok(())
    }
}
/// 逐条建立库存预占、冻结可用量、写预占分录；不预检下一条分配。
pub async fn establish_receipt_reservation(
    db: &Database,
    executor: &mut dyn Executor,
    fact: ReceiptReservationFact<'_>,
) -> Result<()> {
    execute_receipt_reservation(
        &mut ReceiptReservationPosting { store: &mut MongoInventoryStore(db), fact },
        executor,
    )
    .await
}

#[cfg(test)]
mod receipt_write_tests {
    use super::*;
    struct TestExecutor {
        _identity: u8,
    }
    impl Executor for TestExecutor {
        fn session(&mut self) -> Option<&mut mongodb::ClientSession> {
            None
        }
    }
    struct RecordingSteps {
        calls: Vec<&'static str>,
        executor: usize,
        fail_at: Option<&'static str>,
    }
    impl RecordingSteps {
        fn step(&mut self, name: &'static str, executor: &mut dyn Executor) -> Result<()> {
            assert_eq!(executor as *mut dyn Executor as *mut () as usize, self.executor);
            self.calls.push(name);
            if self.fail_at == Some(name) {
                return Err(Error::BusinessLogicError("原库存写入错误".into()));
            }
            Ok(())
        }
    }
    #[async_trait]
    impl ReceiptStockSteps for RecordingSteps {
        async fn balance(&mut self, ex: &mut dyn Executor) -> Result<String> {
            self.step("balance", ex)?;
            Ok("balance-1".into())
        }
        async fn movement(&mut self, ex: &mut dyn Executor) -> Result<String> {
            self.step("movement", ex)?;
            Ok("movement-1".into())
        }
        async fn last_movement(
            &mut self,
            balance: &str,
            movement: &str,
            ex: &mut dyn Executor,
        ) -> Result<()> {
            assert_eq!(balance, "balance-1");
            assert_eq!(movement, "movement-1");
            self.step("lastmovement", ex)
        }
    }
    #[async_trait]
    impl ReceiptReservationSteps for RecordingSteps {
        async fn reservation(&mut self, ex: &mut dyn Executor) -> Result<String> {
            self.step("reservation", ex)?;
            Ok("reservation-1".into())
        }
        async fn reserve(&mut self, ex: &mut dyn Executor) -> Result<()> {
            self.step("reserve", ex)
        }
        async fn entry(&mut self, reservation: &str, ex: &mut dyn Executor) -> Result<()> {
            assert_eq!(reservation, "reservation-1");
            self.step("entry", ex)
        }
    }
    #[tokio::test]
    async fn receipt_stock_preserves_balance_movement_lastmovement_and_executor() {
        let mut ex = TestExecutor { _identity: 1 };
        let mut steps =
            RecordingSteps { calls: vec![], executor: &mut ex as *mut TestExecutor as usize, fail_at: None };
        assert_eq!(execute_receipt_stock(&mut steps, &mut ex).await.unwrap(), "balance-1");
        assert_eq!(steps.calls, ["balance", "movement", "lastmovement"]);
    }
    #[tokio::test]
    async fn receipt_stock_failure_stops_later_writes_with_original_error() {
        let order = ["balance", "movement", "lastmovement"];
        for (index, step) in order.iter().enumerate() {
            let mut ex = TestExecutor { _identity: 1 };
            let mut steps = RecordingSteps {
                calls: vec![],
                executor: &mut ex as *mut TestExecutor as usize,
                fail_at: Some(step),
            };
            assert!(
                matches!(execute_receipt_stock(&mut steps,&mut ex).await,Err(Error::BusinessLogicError(message)) if message=="原库存写入错误")
            );
            assert_eq!(steps.calls, order[..=index]);
        }
    }
    #[tokio::test]
    async fn reservation_preserves_create_reserve_entry_and_executor() {
        let mut ex = TestExecutor { _identity: 1 };
        let mut steps =
            RecordingSteps { calls: vec![], executor: &mut ex as *mut TestExecutor as usize, fail_at: None };
        execute_receipt_reservation(&mut steps, &mut ex).await.unwrap();
        assert_eq!(steps.calls, ["reservation", "reserve", "entry"]);
    }
    #[tokio::test]
    async fn reservation_failure_stops_later_writes_with_original_error() {
        let order = ["reservation", "reserve", "entry"];
        for (index, step) in order.iter().enumerate() {
            let mut ex = TestExecutor { _identity: 1 };
            let mut steps = RecordingSteps {
                calls: vec![],
                executor: &mut ex as *mut TestExecutor as usize,
                fail_at: Some(step),
            };
            assert!(
                matches!(execute_receipt_reservation(&mut steps,&mut ex).await,Err(Error::BusinessLogicError(message)) if message=="原库存写入错误")
            );
            assert_eq!(steps.calls, order[..=index]);
        }
    }
}

#[cfg(test)]
mod receipt_store_tests {
    use std::str::FromStr;

    use super::*;
    struct TestExecutor {
        _identity: u8,
    }
    impl Executor for TestExecutor {
        fn session(&mut self) -> Option<&mut mongodb::ClientSession> {
            None
        }
    }
    struct RecordingStore {
        calls: Vec<&'static str>,
        executor: usize,
        existing: bool,
        fail_at: Option<&'static str>,
        false_at: Option<&'static str>,
        balance: Option<StockBalance>,
        movement: Option<StockMovement>,
        reservation: Option<StockReservation>,
        entry: Option<StockReservationEntry>,
    }
    impl RecordingStore {
        fn new(ex: &mut TestExecutor, existing: bool) -> Self {
            Self {
                calls: vec![],
                executor: ex as *mut TestExecutor as usize,
                existing,
                fail_at: None,
                false_at: None,
                balance: None,
                movement: None,
                reservation: None,
                entry: None,
            }
        }
        fn visit(&mut self, call: &'static str, ex: &mut dyn Executor) -> Result<()> {
            assert_eq!(ex as *mut dyn Executor as *mut () as usize, self.executor);
            self.calls.push(call);
            if self.fail_at == Some(call) {
                return Err(Error::ConflictError("原库存仓储冲突".into()));
            }
            Ok(())
        }
        fn balance_id(&self) -> &str {
            self.balance.as_ref().map(|balance| balance.base.id.as_str()).unwrap_or("balance-1")
        }
    }
    #[async_trait]
    impl InventoryWriteStore for RecordingStore {
        async fn balance(
            &mut self,
            warehouse: &WarehouseId,
            sku: &SkuId,
            ex: &mut dyn Executor,
        ) -> Result<Option<String>> {
            self.visit("balance", ex)?;
            assert_eq!(warehouse.as_ref(), "warehouse-1");
            assert_eq!(sku.as_ref(), "sku-1");
            Ok(self.existing.then(|| "balance-1".into()))
        }
        async fn increase(&mut self, id: &str, quantity: Quantity, ex: &mut dyn Executor) -> Result<bool> {
            self.visit("increase", ex)?;
            assert_eq!(id, "balance-1");
            assert_eq!(quantity, q("2"));
            Ok(self.false_at != Some("increase"))
        }
        async fn create_balance(&mut self, balance: &StockBalance, ex: &mut dyn Executor) -> Result<()> {
            self.visit("create_balance", ex)?;
            self.balance = Some(balance.clone());
            Ok(())
        }
        async fn movement(&mut self, movement: &StockMovement, ex: &mut dyn Executor) -> Result<()> {
            self.visit("movement", ex)?;
            self.movement = Some(movement.clone());
            Ok(())
        }
        async fn last_movement(
            &mut self,
            balance_id: &str,
            movement_id: &str,
            ex: &mut dyn Executor,
        ) -> Result<bool> {
            self.visit("lastmovement", ex)?;
            assert_eq!(balance_id, self.balance_id());
            assert_eq!(movement_id, self.movement.as_ref().unwrap().base.id);
            Ok(self.false_at != Some("lastmovement"))
        }
        async fn create_reservation(
            &mut self,
            reservation: &StockReservation,
            ex: &mut dyn Executor,
        ) -> Result<()> {
            self.visit("reservation", ex)?;
            self.reservation = Some(reservation.clone());
            Ok(())
        }
        async fn reserve(&mut self, id: &str, quantity: Quantity, ex: &mut dyn Executor) -> Result<bool> {
            self.visit("reserve", ex)?;
            assert_eq!(id, self.balance_id());
            assert_eq!(quantity, q("2"));
            Ok(self.false_at != Some("reserve"))
        }
        async fn create_entry(&mut self, entry: &StockReservationEntry, ex: &mut dyn Executor) -> Result<()> {
            self.visit("entry", ex)?;
            self.entry = Some(entry.clone());
            Ok(())
        }
        async fn find_reservation(
            &mut self,
            _id: &str,
            ex: &mut dyn Executor,
        ) -> Result<Option<ReservationFact>> {
            self.visit("find_reservation", ex)?;
            Ok(None)
        }
        async fn consume_reservation(
            &mut self,
            _id: &str,
            _quantity: Quantity,
            ex: &mut dyn Executor,
        ) -> Result<bool> {
            self.visit("consume_reservation", ex)?;
            Ok(false)
        }
        async fn release_reserved(
            &mut self,
            _id: &str,
            _quantity: Quantity,
            ex: &mut dyn Executor,
        ) -> Result<bool> {
            self.visit("release_reserved", ex)?;
            Ok(false)
        }
        async fn deduct_available(
            &mut self,
            _id: &str,
            _quantity: Quantity,
            ex: &mut dyn Executor,
        ) -> Result<bool> {
            self.visit("deduct_available", ex)?;
            Ok(false)
        }
    }
    fn q(value: &str) -> Quantity {
        Quantity::from_str(value).unwrap()
    }
    fn order(existing: bool) -> [&'static str; 7] {
        [
            "balance",
            if existing { "increase" } else { "create_balance" },
            "movement",
            "lastmovement",
            "reservation",
            "reserve",
            "entry",
        ]
    }
    async fn apply(store: &mut RecordingStore, ex: &mut dyn Executor) -> Result<()> {
        let warehouse = WarehouseId::new("warehouse-1");
        let sku = SkuId::new("sku-1");
        let balance_id = execute_receipt_stock(
            &mut ReceiptStockPosting {
                store,
                fact: ReceiptStockFact {
                    warehouse_id: &warehouse,
                    sku_id: &sku,
                    quantity: q("2"),
                    receipt_id: "receipt-1",
                    receipt_line_id: "receipt-line-1",
                },
                occurred_at: Instant::from_unix_secs(12345),
                actor_id: "actor-1",
            },
            ex,
        )
        .await?;
        execute_receipt_reservation(
            &mut ReceiptReservationPosting {
                store,
                fact: ReceiptReservationFact {
                    warehouse_id: &warehouse,
                    sku_id: &sku,
                    sales_order_line_id: SalesOrderLineId::new("sales-line-1"),
                    allocation_id: PurchaseLineSalesAllocationId::new("allocation-1"),
                    receipt_line_id: PurchaseReceiptLineId::new("receipt-line-1"),
                    receipt_id: "receipt-1",
                    balance_id: &balance_id,
                    quantity: q("2"),
                },
            },
            ex,
        )
        .await
    }
    /// 新建和已有余额均使用生产构造器，冻结相同库存来源、数量、时间与预占引用。
    #[tokio::test]
    async fn receipt_existing_and_new_balance_preserve_stock_and_reservation_facts() {
        for existing in [false, true] {
            let mut ex = TestExecutor { _identity: 1 };
            let mut store = RecordingStore::new(&mut ex, existing);
            apply(&mut store, &mut ex).await.unwrap();
            assert_eq!(store.calls, order(existing));
            if let Some(balance) = store.balance.as_ref() {
                assert_eq!(balance.on_hand_quantity, q("2"));
                assert_eq!(balance.available_quantity, q("2"));
                assert_eq!(balance.reserved_quantity, q("0"));
                assert!(balance.last_movement_id.is_none());
            }
            let movement = store.movement.unwrap();
            assert_eq!(movement.source_document_id, "receipt-1");
            assert_eq!(movement.source_line_id.as_deref(), Some("receipt-line-1"));
            assert_eq!(movement.movement_type, MovementType::PurchaseReceiptIn);
            assert_eq!(movement.direction, MovementDirection::Increase);
            assert_eq!(movement.quantity, q("2"));
            assert_eq!(movement.fact.occurred_at, Instant::from_unix_secs(12345));
            assert_eq!(movement.fact.recorded_at, movement.fact.occurred_at);
            assert_eq!(movement.fact.recorded_by, "actor-1");
            assert_eq!(movement.fact.source_type, SourceType::Erp);
            assert!(movement.reversal_of_movement_id.is_none());
            assert!(!movement.base.id.is_empty());
            assert!(!movement.fact.fact_no.is_empty());
            let reservation = store.reservation.unwrap();
            assert_eq!(reservation.sales_order_line_id.as_ref(), "sales-line-1");
            assert_eq!(
                reservation.purchase_line_sales_allocation_id.as_ref().unwrap().as_ref(),
                "allocation-1"
            );
            assert_eq!(reservation.source_receipt_line_id.as_ref().unwrap().as_ref(), "receipt-line-1");
            assert_eq!(reservation.source_type, StockReservationSourceType::PurchaseReceipt);
            assert_eq!(reservation.reserved_quantity, q("2"));
            assert_eq!(reservation.consumed_quantity, q("0"));
            assert_eq!(reservation.released_quantity, q("0"));
            assert_eq!(reservation.status, ReservationStatus::Active);
            let entry = store.entry.unwrap();
            assert_eq!(entry.reservation_id.as_ref(), reservation.base.id);
            assert_eq!(entry.entry_type, ReservationEntryType::Establish);
            assert_eq!(entry.quantity, q("2"));
            assert_eq!(entry.source_document_id, "receipt-1");
            assert_ne!(entry.base.id, reservation.base.id);
        }
    }
    /// 逐个注入真实仓储读写失败，保留原错并停止后续库存写入。
    #[tokio::test]
    async fn receipt_store_failures_stop_all_later_writes() {
        for existing in [false, true] {
            let expected = order(existing);
            for (index, call) in expected.iter().enumerate() {
                let mut ex = TestExecutor { _identity: 1 };
                let mut store = RecordingStore::new(&mut ex, existing);
                store.fail_at = Some(call);
                assert!(
                    matches!(apply(&mut store,&mut ex).await,Err(Error::ConflictError(message)) if message=="原库存仓储冲突")
                );
                assert_eq!(store.calls, expected[..=index]);
            }
        }
    }
    /// 条件写入false必须保持旧错误分类和文案，不执行后续库存事实写入。
    #[tokio::test]
    async fn receipt_conditional_write_failures_keep_original_messages() {
        for (call, message) in [
            ("increase", "库存余额行不存在"),
            ("lastmovement", "库存余额行不存在"),
            ("reserve", "可用库存不足，无法建立销售预占"),
        ] {
            let mut ex = TestExecutor { _identity: 1 };
            let mut store = RecordingStore::new(&mut ex, true);
            store.false_at = Some(call);
            assert!(
                matches!(apply(&mut store,&mut ex).await,Err(Error::BusinessLogicError(found)) if found==message)
            );
            let expected = order(true);
            let index = expected.iter().position(|step| *step == call).unwrap();
            assert_eq!(store.calls, expected[..=index]);
        }
    }
}
