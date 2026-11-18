use std::collections::HashMap;

use application_core::AuditActor;
use erp_core::common::time::Instant;
use mongodb::Database;
use persistence_core::{Executor, Transactional};
use validator::Validate;

use super::InventoryService;
use crate::dto::scope::{MOVEMENT_OWNERSHIP_BASIS, MOVEMENT_SCOPE_SUMMARY};
use crate::dto::{
    InventoryListPage, PageView, StockMovementListParams, StockMovementListQuery, StockMovementView,
    ensure_scope_version,
};
use crate::entity::inventory::{MovementType, StockMovement};
use crate::error::{Error, Result};
use crate::ports::{InventoryAction, WarehouseFactsPort};
use crate::repository::prelude::*;
use crate::repository::{InventoryExt, StockMovementFilter, StockMovementRow};

impl InventoryService {
    /// 分页查询库存流水台账（W10 流水视图，正式事实）。
    ///
    /// # 参数
    /// * `params` - 查询参数（仓库/SKU/类型/方向/发生时间区间/经办人筛选）
    /// * `actor` - 当前认证操作人，用于计算库存流水读取范围
    ///
    /// # 返回
    /// 返回带范围信封的分页视图。
    ///
    /// # 错误
    /// 分页、时间区间、排序、人员或 `scope_version` 非法时返回 `ValidationError`。
    /// 跨页 `scope_version` 不一致时返回 `ConflictError`。
    /// 账号未激活时返回 `Forbidden`。
    /// 授权、商品、仓库、入库单号或仓储查询失败时返回对应错误。
    #[tracing::instrument(
        name = "inventory.stock_movement_list",
        skip_all,
        fields(layer = "service", domain = "inventory", operation = "stock_movement_list")
    )]
    pub async fn stock_movement_list(
        &self,
        params: &StockMovementListParams,
        actor: &AuditActor,
    ) -> Result<InventoryListPage<StockMovementView>> {
        params.validate()?;
        let query = params.normalized()?;
        let service = self.clone();
        let actor = actor.clone();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move { service.search_stock_movements(query, &actor, executor).await })
            })
            .await
    }

    /// 在原快照中读取完整列表、来源单号及仓库名称，最后检查交付。
    async fn search_stock_movements(
        &self,
        mut query: StockMovementListQuery,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<InventoryListPage<StockMovementView>> {
        let authorization =
            self.authorization.authorize(actor, InventoryAction::MovementList, executor).await?;
        if !authorization.actor_is_active() {
            return Err(Error::Forbidden("当前账号无库存流水读取权限".to_string()));
        }
        let search = super::search::sku_filter(
            self.catalog_facts.as_ref(),
            query.q.as_deref(),
            query.sku_id.as_ref(),
            executor,
        )
        .await?;
        let scope_version = query.scope_version.take();
        let filter = movement_filter(query, search);
        let page = self
            .db
            .stock_movements()
            .search_authorized_stock_movements(
                &filter,
                authorization.query(InventoryAction::MovementList),
                executor,
            )
            .await?;
        let meta = authorization.meta(InventoryAction::MovementList);
        ensure_scope_version(filter.page, scope_version.as_deref(), meta.scope_version())?;
        let mut items = page.items.into_iter().map(movement_row_view).collect::<Vec<_>>();
        let numbers =
            load_movement_source_document_nos(&self.db, self.fulfillment_facts.as_ref(), &items, executor)
                .await?;
        hydrate_movement_warehouses(self.warehouse_facts.as_ref(), &mut items, executor).await?;
        for item in &mut items {
            item.source_document_no = numbers.get(&item.source_document_id).cloned();
        }
        let result = InventoryListPage::from_page(
            PageView { items, total: page.total, page: filter.page, page_size: filter.page_size },
            meta,
            authorization.no_scope(InventoryAction::MovementList),
            MOVEMENT_SCOPE_SUMMARY,
            MOVEMENT_OWNERSHIP_BASIS,
        );
        authorization.validate(executor).await?;
        Ok(result)
    }
}

/// 将已规范化的查询字段移动到库存流水筛选条件。
fn movement_filter(
    query: StockMovementListQuery,
    search: crate::repository::InventorySearch,
) -> StockMovementFilter {
    let (sort_by, sort_ascending) = query.paging.sort_selection();
    StockMovementFilter {
        search,
        warehouse_ids: query.warehouse_id.map(|id| vec![id]),
        sku_id: query.sku_id,
        movement_type: query.movement_type,
        direction: query.direction,
        occurred_from: query.occurred_from.map(Instant::from_unix_secs),
        occurred_to: query.occurred_to.map(Instant::from_unix_secs),
        recorded_by_ids: query.operator_user_ids,
        page: query.paging.page,
        page_size: query.paging.page_size,
        sort_by,
        sort_ascending,
    }
}

/// 由流水投影行组装视图；仓库名和来源单号留空，由调用方另补。
///
/// # 参数
/// * `row` - 流水列表投影行。
///
/// # 返回
/// 返回 `warehouse_name` 与 `source_document_no` 均为 `None` 的视图。
///
/// # 错误
/// 不返回错误。
pub(super) fn movement_row_view(row: StockMovementRow) -> StockMovementView {
    StockMovementView {
        warehouse_name: None,
        id: row.id,
        warehouse_id: row.warehouse_id.to_string(),
        sku_id: row.sku_id.to_string(),
        movement_type: row.movement_type,
        direction: row.direction,
        quantity: row.quantity,
        source_document_id: row.source_document_id,
        source_document_no: None,
        source_line_id: row.source_line_id,
        occurred_at: row.occurred_at.unix_secs(),
        recorded_at: row.recorded_at.unix_secs(),
        recorded_by: row.recorded_by,
    }
}

/// 批量解析流水来源单据号（按流水类型查对应单据集合，`$in` 批量避免 N+1）。
///
/// 当前支持库存调整单（盘盈/盘亏/损坏）与采购入库单；其余来源类型暂不解析，
/// 由前端回退显示来源主键。
///
/// # 参数
/// * `db` - 数据库实例。
/// * `fulfillment` - 采购入库单号端口。
/// * `movements` - 流水视图列表。
///
/// # 返回
/// 返回「来源单据主键 → 单据号」映射。未解析的来源不出现在映射中。
///
/// # 错误
/// 调整单查询或入库单号读取失败时返回对应错误。
pub(super) async fn load_movement_source_document_nos(
    db: &Database,
    fulfillment: &dyn crate::ports::FulfillmentFactsPort,
    movements: &[StockMovementView],
    executor: &mut dyn Executor,
) -> Result<HashMap<String, String>> {
    let mut document_nos = HashMap::new();
    let adjustment_ids: Vec<String> = movements
        .iter()
        .filter(|movement| {
            matches!(
                movement.movement_type,
                MovementType::StockGain | MovementType::StockLoss | MovementType::Damage
            )
        })
        .map(|movement| movement.source_document_id.clone())
        .collect();
    if !adjustment_ids.is_empty() {
        let adjustments = db.inventory().stock_adjustments_by_ids(&adjustment_ids, executor).await?;
        for adjustment in adjustments {
            document_nos.insert(adjustment.base.id, adjustment.adjustment_no);
        }
    }
    let receipt_ids: Vec<String> = movements
        .iter()
        .filter(|movement| matches!(movement.movement_type, MovementType::PurchaseReceiptIn))
        .map(|movement| movement.source_document_id.clone())
        .collect();
    if !receipt_ids.is_empty() {
        let receipts = fulfillment.receipt_nos_by_ids(&receipt_ids, executor).await?;
        for (id, receipt) in receipts {
            document_nos.insert(id, receipt.receipt_no);
        }
    }
    Ok(document_nos)
}

impl From<StockMovement> for StockMovementView {
    /// 从流水实体构造视图。
    fn from(movement: StockMovement) -> Self {
        movement_row_view(StockMovementRow {
            id: movement.base.id,
            warehouse_id: movement.warehouse_id,
            sku_id: movement.sku_id,
            movement_type: movement.movement_type,
            direction: movement.direction,
            quantity: movement.quantity,
            source_document_id: movement.source_document_id,
            source_line_id: movement.source_line_id,
            occurred_at: movement.fact.occurred_at,
            recorded_at: movement.fact.recorded_at,
            recorded_by: movement.fact.recorded_by,
        })
    }
}

/// 补齐已授权结果页的仓库名称，不产生目录候选或扩大流水范围。
async fn hydrate_movement_warehouses(
    warehouse: &dyn WarehouseFactsPort,
    items: &mut [StockMovementView],
    executor: &mut dyn Executor,
) -> Result<()> {
    if items.is_empty() {
        return Ok(());
    }
    let ids = items.iter().map(|row| row.warehouse_id.clone()).collect::<Vec<_>>();
    let warehouses = warehouse.warehouses_by_ids(&ids, executor).await?;
    let revision_ids =
        warehouses.values().filter_map(|row| row.current_revision_id.clone()).collect::<Vec<_>>();
    let revisions = warehouse.warehouse_revisions_by_ids(&revision_ids, executor).await?;
    for item in items {
        item.warehouse_name = warehouses.get(&item.warehouse_id).map(|warehouse| {
            warehouse
                .current_revision_id
                .as_ref()
                .and_then(|id| revisions.get(id))
                .map(|revision| revision.name.clone())
                .unwrap_or_else(|| warehouse.warehouse_code.clone())
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use async_trait::async_trait;
    use erp_core::ids::{SkuId, WarehouseId};
    use persistence_core::testing::SnapshotExecutor;

    use super::*;
    use crate::entity::inventory::MovementDirection;
    use crate::ports::{WarehouseFact, WarehouseRevisionFact};

    struct Warehouses {
        executor_id: usize,
        fail_revision: bool,
    }

    impl Warehouses {
        fn check_executor(&self, executor: &mut dyn Executor) {
            assert_eq!(executor as *mut dyn Executor as *mut () as usize, self.executor_id);
            assert!(executor.boundary().unwrap().is_snapshot());
        }
    }

    #[async_trait]
    impl WarehouseFactsPort for Warehouses {
        async fn warehouse_exists(&self, _: &str, _: &mut dyn Executor) -> Result<bool> {
            panic!("展示不得查询仓库候选");
        }
        async fn warehouses_by_ids(
            &self,
            ids: &[String],
            executor: &mut dyn Executor,
        ) -> Result<HashMap<String, WarehouseFact>> {
            self.check_executor(executor);
            assert_eq!(ids, ["warehouse"]);
            Ok([(
                "warehouse".into(),
                WarehouseFact {
                    id: "warehouse".into(),
                    warehouse_code: "WH-1".into(),
                    current_revision_id: Some("revision".into()),
                },
            )]
            .into())
        }
        async fn warehouse_revisions_by_ids(
            &self,
            ids: &[String],
            executor: &mut dyn Executor,
        ) -> Result<HashMap<String, WarehouseRevisionFact>> {
            self.check_executor(executor);
            assert_eq!(ids, ["revision"]);
            if self.fail_revision {
                return Err(Error::Internal("revision-unavailable".into()));
            }
            Ok([(
                "revision".into(),
                WarehouseRevisionFact { id: "revision".into(), name: "原快照仓库".into() },
            )]
            .into())
        }
    }

    #[tokio::test]
    async fn warehouse_display_uses_original_snapshot_and_propagates_read_failure() {
        for fail_revision in [false, true] {
            let mut executor = SnapshotExecutor::default();
            let warehouse =
                Warehouses { executor_id: &mut executor as *mut SnapshotExecutor as usize, fail_revision };
            let mut items = vec![movement_row_view(StockMovementRow {
                id: "movement".into(),
                warehouse_id: WarehouseId::new("warehouse"),
                sku_id: SkuId::new("sku"),
                movement_type: MovementType::StockGain,
                direction: MovementDirection::Increase,
                quantity: "1".parse().unwrap(),
                source_document_id: "adjustment".into(),
                source_line_id: None,
                occurred_at: Instant::now(),
                recorded_at: Instant::now(),
                recorded_by: "actor".into(),
            })];
            let result = hydrate_movement_warehouses(&warehouse, &mut items, &mut executor).await;
            if fail_revision {
                assert!(matches!(result, Err(Error::Internal(message)) if message == "revision-unavailable"));
                assert!(items[0].warehouse_name.is_none());
            } else {
                result.unwrap();
                assert_eq!(items[0].warehouse_name.as_deref(), Some("原快照仓库"));
            }
        }
    }
}
