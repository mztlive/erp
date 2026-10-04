//! 现有库存选源的原子预占写入边界。
use std::collections::HashMap;
use std::str::FromStr;

use async_trait::async_trait;
use erp_core::ids::{SalesOrderId, SalesOrderLineId, StockReservationEntryId, StockReservationId};
use erp_core::money::Quantity;
use erp_inventory::repository::prelude::*;
use erp_inventory::{
    InventoryExt, ReservationEntryType, ReservationStatus, StockReservation, StockReservationData,
    StockReservationEntry, StockReservationEntryData, StockReservationSourceType,
};
use erp_procurement::dto::purchase_order::ExistingStockReservationResult;
use erp_procurement::entity::purchase_order::{
    RequestedStockLine, StockAllocationPlan, StockBasisGroup, payload_fingerprint,
};
use id_generator::next_id;
use mongodb::Database;
use persistence_core::Executor;

use super::{latest_stock_group, procurement_quantity_changed};
use crate::{Error, Result};

/// 已持久化的现有库存分配及其公开结果。
pub(super) struct PersistedStockAllocation {
    /// 新建库存预占。
    pub(super) reservation: StockReservation,
    /// API 返回投影。
    pub(super) result: ExistingStockReservationResult,
}

/// 实际库存写端口；业务规则与 ID 构造仍按原函数时点执行。
///
/// 预占按余额汇总一次 CAS，预占与分录各一次批量插入；事务内只保留最小写入集。
#[async_trait]
trait StockAllocationPort: Send + Sync {
    /// 在调用方事务内按可用数量条件增加预占。
    async fn reserve_quantity(
        &self,
        balance_id: &str,
        quantity: Quantity,
        executor: &mut dyn Executor,
    ) -> Result<bool>;
    /// 按计划顺序保存全部预占实体。
    async fn create_reservations(
        &self,
        reservations: &[StockReservation],
        executor: &mut dyn Executor,
    ) -> Result<()>;
    /// 在同一事务内保存对应的预占分录。
    async fn create_entries(
        &self,
        entries: &[StockReservationEntry],
        executor: &mut dyn Executor,
    ) -> Result<()>;
}
struct StockAllocationAdapter<'a> {
    db: &'a Database,
}
#[async_trait]
impl StockAllocationPort for StockAllocationAdapter<'_> {
    /// 复用库存领域仓储的余额 CAS。
    async fn reserve_quantity(
        &self,
        id: &str,
        quantity: Quantity,
        executor: &mut dyn Executor,
    ) -> Result<bool> {
        Ok(self.db.stock_balances().reserve_quantity(id, quantity, executor).await?)
    }
    /// 经库存领域仓储批量保存预占，不直接操作集合。
    async fn create_reservations(
        &self,
        reservations: &[StockReservation],
        executor: &mut dyn Executor,
    ) -> Result<()> {
        self.db.inventory().create_reservations(reservations, executor).await?;
        Ok(())
    }
    /// 经库存领域仓储批量保存分录，并传递同一执行器。
    async fn create_entries(
        &self,
        entries: &[StockReservationEntry],
        executor: &mut dyn Executor,
    ) -> Result<()> {
        self.db.inventory().create_reservation_entries(entries, executor).await?;
        Ok(())
    }
}
/// 绑定实际库存仓储并复用原事务，不能提前查询或申请新会话。
///
/// 调用方在同一事务内重验库存依据；本函数汇总数量后执行 CAS 与批量插入。
///
/// # 参数
/// * `db` - 组合层数据库句柄
/// * `plans` - 已校验的库存分配计划
/// * `latest_groups` - 同一事务内重验的库存依据
/// * `sales_order_id` - 来源销售单
/// * `command_id` - 命令收据 ID
/// * `request_fingerprint` - 命令载荷指纹
/// * `executor` - 调用方事务执行器
///
/// # 返回
/// 按计划行顺序保存的预占及公开结果。
///
/// # 错误
/// 库存依据失效、可用量不足或仓储写入失败时拒绝。
pub(super) async fn persist_stock_allocations(
    db: &Database,
    plans: &[StockAllocationPlan],
    latest_groups: &[StockBasisGroup],
    sales_order_id: &SalesOrderId,
    command_id: &str,
    request_fingerprint: &str,
    executor: &mut dyn Executor,
) -> Result<Vec<PersistedStockAllocation>> {
    persist_with_port(
        &StockAllocationAdapter { db },
        plans,
        latest_groups,
        sales_order_id,
        command_id,
        request_fingerprint,
        executor,
    )
    .await
}

/// 先完成全部余额 CAS，再保存预占和分录；任一步失败立即停止。
async fn persist_with_port(
    port: &dyn StockAllocationPort,
    plans: &[StockAllocationPlan],
    latest_groups: &[StockBasisGroup],
    sales_order_id: &SalesOrderId,
    command_id: &str,
    request_fingerprint: &str,
    executor: &mut dyn Executor,
) -> Result<Vec<PersistedStockAllocation>> {
    let pending =
        build_pending_allocations(plans, latest_groups, sales_order_id, command_id, request_fingerprint)?;
    for (balance_id, total) in &pending.totals {
        if !port.reserve_quantity(balance_id, *total, executor).await? {
            return Err(procurement_quantity_changed());
        }
    }
    port.create_reservations(&pending.reservations, executor).await?;
    port.create_entries(&pending.entries, executor).await?;
    Ok(pending.persisted)
}

/// 待写入的预占与分录及其按余额汇总的预占总量。
struct PendingAllocations {
    /// 新建预占实体，按行顺序排列。
    reservations: Vec<StockReservation>,
    /// 新建分录实体，与预占一一对应。
    entries: Vec<StockReservationEntry>,
    /// 按余额汇总的预占总量，余额只 CAS 一次。
    totals: Vec<(String, Quantity)>,
    /// 对外返回的持久化结果。
    persisted: Vec<PersistedStockAllocation>,
}

/// 组装全部预占与分录并按余额汇总总量（纯内存，不访问数据库）。
///
/// # 参数
/// * `plans` - 现有库存分配计划
/// * `latest_groups` - 最新库存余额依据
/// * `sales_order_id` - 来源销售单
/// * `command_id` - 命令收据 ID
/// * `request_fingerprint` - 命令载荷指纹
///
/// # 返回
/// 返回待批量写入的预占、分录、汇总总量与结果投影。
///
/// # 错误
/// 余额依据失效、行缺失或实体构造失败时返回错误。
fn build_pending_allocations(
    plans: &[StockAllocationPlan],
    latest_groups: &[StockBasisGroup],
    sales_order_id: &SalesOrderId,
    command_id: &str,
    request_fingerprint: &str,
) -> Result<PendingAllocations> {
    let zero = Quantity::from_str("0").map_err(Error::Logic)?;
    let mut pending = PendingAllocations {
        reservations: Vec::new(),
        entries: Vec::new(),
        totals: Vec::new(),
        persisted: Vec::new(),
    };
    let mut sums: HashMap<String, Quantity> = HashMap::new();
    for plan in plans {
        let latest = latest_stock_group(latest_groups, &plan.group.balance.base.id)?;
        for requested in &plan.requested_lines {
            let built = build_pending_line(
                latest,
                requested,
                &zero,
                sales_order_id,
                command_id,
                request_fingerprint,
            )?;
            sums.insert(built.0.clone(), sum_quantity(sums.get(&built.0).copied().unwrap_or(zero), built.1)?);
            pending.reservations.push(built.2);
            pending.entries.push(built.3);
            pending.persisted.push(built.4);
        }
    }
    let mut totals: Vec<(String, Quantity)> = sums.into_iter().collect();
    totals.sort_by(|left, right| left.0.cmp(&right.0));
    pending.totals = totals;
    Ok(pending)
}

/// 组装单行的预占、分录与结果投影（纯内存）。
///
/// # 参数
/// * `latest` - 命中的最新余额依据
/// * `requested` - 本余额逐销售行分配数量
/// * `zero` - 零数量
/// * `sales_order_id` - 来源销售单
/// * `command_id` - 命令收据 ID
/// * `request_fingerprint` - 命令载荷指纹
///
/// # 返回
/// 返回（余额 ID，数量，预占，分录，结果投影）。
///
/// # 错误
/// 行缺失或实体构造失败时返回错误。
fn build_pending_line(
    latest: &StockBasisGroup,
    requested: &RequestedStockLine,
    zero: &Quantity,
    sales_order_id: &SalesOrderId,
    command_id: &str,
    request_fingerprint: &str,
) -> Result<(String, Quantity, StockReservation, StockReservationEntry, PersistedStockAllocation)> {
    let line = latest.line_for(&requested.sales_order_line_id).ok_or_else(procurement_quantity_changed)?;
    let source_allocation_id = payload_fingerprint(
        "inventory.allocate_existing_stock",
        sales_order_id.as_ref(),
        &(
            request_fingerprint,
            latest.balance.base.id.as_str(),
            requested.sales_order_line_id.as_str(),
            requested.quantity.to_string(),
        ),
    )?;
    let reservation = StockReservation::new(
        StockReservationId::new(next_id()),
        StockReservationData {
            warehouse_id: latest.balance.warehouse_id.clone(),
            sku_id: line.coverage.goods_line.sku_id.clone(),
            sales_order_line_id: SalesOrderLineId::new(requested.sales_order_line_id.clone()),
            source_type: StockReservationSourceType::ExistingStock,
            purchase_line_sales_allocation_id: None,
            source_receipt_line_id: None,
            source_allocation_id: Some(source_allocation_id),
            reserved_quantity: requested.quantity,
            consumed_quantity: *zero,
            released_quantity: *zero,
            status: ReservationStatus::Active,
        },
    )?;
    let entry = StockReservationEntry::new(
        StockReservationEntryId::new(next_id()),
        StockReservationEntryData {
            reservation_id: reservation.base.id.clone().into(),
            entry_type: ReservationEntryType::Establish,
            quantity: requested.quantity,
            source_document_id: command_id.to_string(),
        },
    )?;
    let result = ExistingStockReservationResult {
        stock_reservation_id: reservation.base.id.clone(),
        sales_order_line_id: requested.sales_order_line_id.clone(),
        stock_balance_id: latest.balance.base.id.clone(),
        warehouse_id: latest.balance.warehouse_id.to_string(),
        quantity: requested.quantity.to_string(),
    };
    let persisted = PersistedStockAllocation { reservation: reservation.clone(), result };
    Ok((latest.balance.base.id.clone(), requested.quantity, reservation, entry, persisted))
}

/// 定点数量精确相加，不做舍入。
///
/// # 参数
/// * `current` - 当前累计
/// * `add` - 本次增量
///
/// # 返回
/// 返回相加后的数量。
///
/// # 错误
/// 精度溢出时返回逻辑错误。
fn sum_quantity(current: Quantity, add: Quantity) -> Result<Quantity> {
    Quantity::try_from(current.to_decimal() + add.to_decimal()).map_err(Error::Logic)
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use erp_core::common::time::Instant;
    use erp_core::ids::{SalesOrderRevisionLineId, SkuId, SkuRevisionId, WarehouseId};
    use erp_procurement::entity::facts::{
        FactIdentity, ProductKind, SalesCustomerSnapshotFact, SalesGoodsLineFact, SalesLineType,
        SalesRevisionFact, SalesRevisionLineFact, StockBalanceFact, VersionedFactIdentity,
    };
    use erp_procurement::entity::purchase_order::{
        ProcurementCoverageSummary, RequestedStockLine, SalesProcurementCoverageLine, StockBasisLine,
    };
    use mongodb::ClientSession;

    use super::*;

    struct RecordingExecutor {
        marker: u64,
    }
    impl Executor for RecordingExecutor {
        fn session(&mut self) -> Option<&mut ClientSession> {
            self.marker += 1;
            None
        }
    }
    #[derive(Default)]
    struct Calls {
        steps: Vec<&'static str>,
        reservation_ids: Vec<String>,
    }
    struct RecordingPort {
        executor: usize,
        fail_at: Option<usize>,
        cas_miss: bool,
        calls: Mutex<Calls>,
    }
    impl RecordingPort {
        fn record(&self, step: &'static str, executor: &mut dyn Executor) -> Result<()> {
            assert_eq!(executor as *mut dyn Executor as *mut () as usize, self.executor);
            let mut calls = self.calls.lock().unwrap();
            let index = calls.steps.len();
            calls.steps.push(step);
            if self.fail_at == Some(index) {
                return Err(Error::Internal(format!("stock failure at {index}")));
            }
            Ok(())
        }
    }
    #[async_trait]
    impl StockAllocationPort for RecordingPort {
        async fn reserve_quantity(
            &self,
            id: &str,
            quantity: Quantity,
            executor: &mut dyn Executor,
        ) -> Result<bool> {
            self.record("reserve", executor)?;
            assert_eq!(id, "balance-1");
            assert_eq!(quantity, q("5"));
            Ok(!self.cas_miss)
        }
        async fn create_reservations(
            &self,
            reservations: &[StockReservation],
            executor: &mut dyn Executor,
        ) -> Result<()> {
            self.record("reservations", executor)?;
            assert_eq!(reservations.len(), 2);
            for reservation in reservations {
                assert_eq!(reservation.source_type, StockReservationSourceType::ExistingStock);
                assert_eq!(reservation.warehouse_id.as_ref(), "warehouse-1");
                assert_eq!(reservation.sku_id.as_ref(), "sku-1");
                assert_eq!(reservation.consumed_quantity, q("0"));
                assert_eq!(reservation.released_quantity, q("0"));
                assert!(reservation.source_allocation_id.is_some());
                self.calls.lock().unwrap().reservation_ids.push(reservation.base.id.clone());
            }
            Ok(())
        }
        async fn create_entries(
            &self,
            entries: &[StockReservationEntry],
            executor: &mut dyn Executor,
        ) -> Result<()> {
            self.record("entries", executor)?;
            assert_eq!(entries.len(), 2);
            let ids = self.calls.lock().unwrap().reservation_ids.clone();
            assert_eq!(ids.len(), 2);
            for entry in entries {
                assert_eq!(entry.entry_type, ReservationEntryType::Establish);
                assert_eq!(entry.source_document_id, "purchase-command-1");
                assert!(ids.iter().any(|id| Some(id.as_str()) == Some(entry.reservation_id.as_ref())));
            }
            Ok(())
        }
    }
    fn q(value: &str) -> Quantity {
        Quantity::from_str(value).unwrap()
    }
    fn line(id: &str, no: u32) -> StockBasisLine {
        StockBasisLine {
            max_create_quantity: q("5"),
            coverage: SalesProcurementCoverageLine {
                quantity_scale: Some(6),
                revision_line: SalesRevisionLineFact {
                    base: FactIdentity { id: format!("revision-{id}") },
                    sales_order_line_id: SalesOrderLineId::new(id),
                    line_no: no,
                    line_type: SalesLineType::GoodsService,
                    item_name_snapshot: "茶叶".to_string(),
                    spec_snapshot: None,
                    unit_snapshot: Some("盒".to_string()),
                },
                goods_line: SalesGoodsLineFact {
                    revision_line_id: SalesOrderRevisionLineId::new(format!("revision-{id}")),
                    sku_id: SkuId::new("sku-1"),
                    sku_revision_id: SkuRevisionId::new("sku-revision-1"),
                    quantity: q("5"),
                    base_unit_code: "BOX".to_string(),
                    fulfillment_due_at: Instant::from_unix_secs(1_800_000_000),
                },
                product_kind: ProductKind::Physical,
                summary: ProcurementCoverageSummary::new(q("5"), q("0")).unwrap(),
            },
        }
    }
    fn plan() -> StockAllocationPlan {
        StockAllocationPlan {
            group: StockBasisGroup {
                revision: SalesRevisionFact {
                    base: FactIdentity { id: "sales-revision-1".to_string() },
                    customer_snapshot: SalesCustomerSnapshotFact { customer_name: "客户".to_string() },
                    contract_snapshot: None,
                },
                balance: StockBalanceFact {
                    base: VersionedFactIdentity { id: "balance-1".to_string(), version: 7 },
                    warehouse_id: WarehouseId::new("warehouse-1"),
                    sku_id: SkuId::new("sku-1"),
                    available_quantity: q("10"),
                },
                warehouse_name: "仓库".to_string(),
                lines: vec![line("line-1", 1), line("line-2", 2)],
            },
            requested_lines: vec![
                RequestedStockLine { sales_order_line_id: "line-1".to_string(), quantity: q("2") },
                RequestedStockLine { sales_order_line_id: "line-2".to_string(), quantity: q("3") },
            ],
        }
    }
    async fn invoke(
        fail_at: Option<usize>,
        cas_miss: bool,
    ) -> (Result<Vec<PersistedStockAllocation>>, Vec<&'static str>) {
        let mut executor = RecordingExecutor { marker: 73 };
        let port = RecordingPort {
            executor: (&mut executor as *mut RecordingExecutor) as usize,
            fail_at,
            cas_miss,
            calls: Mutex::default(),
        };
        let plan = plan();
        let latest = vec![plan.group.clone()];
        let result = persist_with_port(
            &port,
            &[plan],
            &latest,
            &SalesOrderId::new("sales-1"),
            "purchase-command-1",
            "fingerprint-1",
            &mut executor,
        )
        .await;
        assert_eq!(executor.marker, 73);
        (result, port.calls.into_inner().unwrap().steps)
    }
    /// 同一余额两行汇总一次 CAS 后批量写预占与分录，保持同一非零大小执行器。
    #[tokio::test]
    async fn stock_posting_uses_same_executor_and_original_write_order() {
        let (result, calls) = invoke(None, false).await;
        let persisted = result.unwrap();
        assert_eq!(calls, vec!["reserve", "reservations", "entries"]);
        assert_eq!(persisted.len(), 2);
        assert_eq!(persisted[0].result.sales_order_line_id, "line-1");
        assert_eq!(persisted[0].result.quantity, q("2").to_string());
        assert_eq!(persisted[1].result.sales_order_line_id, "line-2");
        assert_eq!(persisted[1].result.quantity, q("3").to_string());
    }
    /// 每一写入失败均原样传播，停止剩余步骤和下一条分配。
    #[tokio::test]
    async fn stock_posting_stops_at_each_failed_write() {
        let sequence = ["reserve", "reservations", "entries"];
        for index in 0..sequence.len() {
            let (result, calls) = invoke(Some(index), false).await;
            assert!(
                matches!(result,Err(Error::Internal(message)) if message==format!("stock failure at {index}"))
            );
            assert_eq!(calls, sequence[..=index]);
        }
    }
    /// 余额 CAS 未命中保持统一可刷新冲突，不能先写预占或分录。
    #[tokio::test]
    async fn stock_reservation_cas_miss_stops_before_persisting_reservation() {
        let (result, calls) = invoke(None, true).await;
        assert!(
            matches!(result,Err(Error::ConflictError(message)) if message=="可分配供给数量已更新，请刷新后重试")
        );
        assert_eq!(calls, vec!["reserve"]);
    }
}
