//! 现有库存选源的原子预占写入边界。
use super::{latest_stock_group, procurement_quantity_changed};
use crate::{Error, Result};
use async_trait::async_trait;
use erp_core::ids::{SalesOrderId, SalesOrderLineId, StockReservationEntryId, StockReservationId};
use erp_core::money::Quantity;
use erp_inventory::{
    InventoryExt, ReservationEntryType, ReservationStatus, StockReservation, StockReservationData,
    StockReservationEntry, StockReservationEntryData, StockReservationSourceType,
};
use erp_procurement::dto::purchase_order::ExistingStockReservationResult;
use erp_procurement::entity::purchase_order::{payload_fingerprint, StockAllocationPlan, StockBasisGroup};
use id_generator::next_id;
use mongodb::{ClientSession, Database};
use persistence_core::Executor;
use std::str::FromStr;

/// 已持久化的现有库存分配及其公开结果。
pub(super) struct PersistedStockAllocation {
    /// 新建库存预占。
    pub(super) reservation: StockReservation,
    /// API 返回投影。
    pub(super) result: ExistingStockReservationResult,
}

/// 实际库存写端口；业务规则与 ID 构造仍按原函数时点执行。
#[async_trait]
trait StockAllocationPort: Send + Sync {
    async fn reserve_quantity(
        &self,
        balance_id: &str,
        quantity: Quantity,
        executor: &mut dyn Executor,
    ) -> Result<bool>;
    async fn create_reservation(
        &self,
        reservation: &StockReservation,
        executor: &mut dyn Executor,
    ) -> Result<()>;
    async fn create_entry(&self, entry: &StockReservationEntry, executor: &mut dyn Executor) -> Result<()>;
}
struct StockAllocationAdapter<'a> {
    db: &'a Database,
}
#[async_trait]
impl StockAllocationPort for StockAllocationAdapter<'_> {
    async fn reserve_quantity(
        &self,
        id: &str,
        quantity: Quantity,
        executor: &mut dyn Executor,
    ) -> Result<bool> {
        Ok(self
            .db
            .stock_balances()
            .reserve_quantity(id, quantity, executor)
            .await?)
    }
    async fn create_reservation(
        &self,
        reservation: &StockReservation,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        self.db.stock_reservations().create(reservation, executor).await?;
        Ok(())
    }
    async fn create_entry(&self, entry: &StockReservationEntry, executor: &mut dyn Executor) -> Result<()> {
        self.db
            .stock_reservation_entries()
            .create(entry, executor)
            .await?;
        Ok(())
    }
}
/// 绑定实际库存仓储并复用原事务，不能提前查询或申请新会话。
pub(super) async fn persist_stock_allocations(
    db: &Database,
    plans: &[StockAllocationPlan],
    latest_groups: &[StockBasisGroup],
    sales_order_id: &SalesOrderId,
    audit_id: &str,
    request_fingerprint: &str,
    session: &mut ClientSession,
) -> Result<Vec<PersistedStockAllocation>> {
    persist_with_port(
        &StockAllocationAdapter { db },
        plans,
        latest_groups,
        sales_order_id,
        audit_id,
        request_fingerprint,
        session,
    )
    .await
}

async fn persist_with_port(
    port: &dyn StockAllocationPort,
    plans: &[StockAllocationPlan],
    latest_groups: &[StockBasisGroup],
    sales_order_id: &SalesOrderId,
    audit_id: &str,
    request_fingerprint: &str,
    executor: &mut dyn Executor,
) -> Result<Vec<PersistedStockAllocation>> {
    let zero = Quantity::from_str("0").map_err(Error::Logic)?;
    let mut persisted = Vec::new();
    for plan in plans {
        let latest = latest_stock_group(latest_groups, &plan.group.balance.base.id)?;
        for requested in &plan.requested_lines {
            let line = latest
                .line_for(&requested.sales_order_line_id)
                .ok_or_else(procurement_quantity_changed)?;
            if !port
                .reserve_quantity(&latest.balance.base.id, requested.quantity, executor)
                .await?
            {
                return Err(procurement_quantity_changed());
            }
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
                    consumed_quantity: zero,
                    released_quantity: zero,
                    status: ReservationStatus::Active,
                },
            )?;
            port.create_reservation(&reservation, executor).await?;
            let entry = StockReservationEntry::new(
                StockReservationEntryId::new(next_id()),
                StockReservationEntryData {
                    reservation_id: reservation.base.id.clone().into(),
                    entry_type: ReservationEntryType::Establish,
                    quantity: requested.quantity,
                    source_document_id: audit_id.to_string(),
                },
            )?;
            port.create_entry(&entry, executor).await?;
            persisted.push(PersistedStockAllocation {
                result: ExistingStockReservationResult {
                    stock_reservation_id: reservation.base.id.clone(),
                    sales_order_line_id: requested.sales_order_line_id.clone(),
                    stock_balance_id: latest.balance.base.id.clone(),
                    warehouse_id: latest.balance.warehouse_id.to_string(),
                    quantity: requested.quantity.to_string(),
                },
                reservation,
            });
        }
    }
    Ok(persisted)
}

#[cfg(test)]
mod tests {
    use super::*;
    use erp_core::common::time::Instant;
    use erp_core::ids::{SalesOrderRevisionLineId, SkuId, SkuRevisionId, WarehouseId};
    use erp_procurement::entity::facts::{
        FactIdentity, ProductKind, SalesCustomerSnapshotFact, SalesGoodsLineFact, SalesLineType,
        SalesRevisionFact, SalesRevisionLineFact, StockBalanceFact, VersionedFactIdentity,
    };
    use erp_procurement::entity::purchase_order::{
        ProcurementCoverageSummary, RequestedStockLine, SalesProcurementCoverageLine, StockBasisLine,
    };
    use std::sync::Mutex;

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
        last_reservation_id: Option<String>,
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
            assert!(quantity == q("2") || quantity == q("3"));
            Ok(!self.cas_miss)
        }
        async fn create_reservation(
            &self,
            reservation: &StockReservation,
            executor: &mut dyn Executor,
        ) -> Result<()> {
            self.record("reservation", executor)?;
            assert_eq!(reservation.source_type, StockReservationSourceType::ExistingStock);
            assert_eq!(reservation.warehouse_id.as_ref(), "warehouse-1");
            assert_eq!(reservation.sku_id.as_ref(), "sku-1");
            assert_eq!(reservation.consumed_quantity, q("0"));
            assert_eq!(reservation.released_quantity, q("0"));
            assert!(reservation.source_allocation_id.is_some());
            self.calls.lock().unwrap().last_reservation_id = Some(reservation.base.id.clone());
            Ok(())
        }
        async fn create_entry(
            &self,
            entry: &StockReservationEntry,
            executor: &mut dyn Executor,
        ) -> Result<()> {
            self.record("entry", executor)?;
            assert_eq!(entry.entry_type, ReservationEntryType::Establish);
            assert_eq!(entry.source_document_id, "audit-1");
            assert_eq!(
                Some(entry.reservation_id.as_ref()),
                self.calls.lock().unwrap().last_reservation_id.as_deref()
            );
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
                    base: FactIdentity {
                        id: format!("revision-{id}"),
                    },
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
                    base: FactIdentity {
                        id: "sales-revision-1".to_string(),
                    },
                    customer_snapshot: SalesCustomerSnapshotFact {
                        customer_name: "客户".to_string(),
                    },
                    contract_snapshot: None,
                },
                balance: StockBalanceFact {
                    base: VersionedFactIdentity {
                        id: "balance-1".to_string(),
                        version: 7,
                    },
                    warehouse_id: WarehouseId::new("warehouse-1"),
                    sku_id: SkuId::new("sku-1"),
                    available_quantity: q("10"),
                },
                warehouse_name: "仓库".to_string(),
                lines: vec![line("line-1", 1), line("line-2", 2)],
            },
            requested_lines: vec![
                RequestedStockLine {
                    sales_order_line_id: "line-1".to_string(),
                    quantity: q("2"),
                },
                RequestedStockLine {
                    sales_order_line_id: "line-2".to_string(),
                    quantity: q("3"),
                },
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
            "audit-1",
            "fingerprint-1",
            &mut executor,
        )
        .await;
        assert_eq!(executor.marker, 73);
        (result, port.calls.into_inner().unwrap().steps)
    }
    /// 两条分配均依次预占余额、写预占、写分录，并保持同一非零大小执行器。
    #[tokio::test]
    async fn stock_posting_uses_same_executor_and_original_write_order() {
        let (result, calls) = invoke(None, false).await;
        let persisted = result.unwrap();
        assert_eq!(
            calls,
            vec![
                "reserve",
                "reservation",
                "entry",
                "reserve",
                "reservation",
                "entry"
            ]
        );
        assert_eq!(persisted.len(), 2);
        assert_eq!(persisted[0].result.sales_order_line_id, "line-1");
        assert_eq!(persisted[0].result.quantity, q("2").to_string());
        assert_eq!(persisted[1].result.sales_order_line_id, "line-2");
        assert_eq!(persisted[1].result.quantity, q("3").to_string());
    }
    /// 每一写入失败均原样传播，停止剩余步骤和下一条分配。
    #[tokio::test]
    async fn stock_posting_stops_at_each_failed_write() {
        let sequence = [
            "reserve",
            "reservation",
            "entry",
            "reserve",
            "reservation",
            "entry",
        ];
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
