use std::collections::HashMap;

use application_core::AuditActor;
use erp_core::common::time::Instant;
use mongodb::Database;
use persistence_core::{NoTransaction, Transactional};
use validator::Validate;

use super::InventoryService;
use crate::dto::scope::{MOVEMENT_OWNERSHIP_BASIS, MOVEMENT_SCOPE_SUMMARY};
use crate::dto::{
    InventoryListPage, PageView, StockMovementListParams, StockMovementListQuery, StockMovementView,
    ensure_scope_version,
};
use crate::entity::inventory::{MovementType, StockMovement};
use crate::error::{Error, Result};
use crate::ports::InventoryAuthorization;
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
    /// * `ValidationError` - 分页/时间区间/排序/人员参数非法
    /// * `ConflictError` - 跨页 `scope_version` 不一致
    /// * `RepositoryError` - 数据库查询失败
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
        let (page, authorization) = self.search_stock_movements(&query, actor).await?;
        let meta = authorization.movement_list_meta();
        ensure_scope_version(query.paging.page, query.scope_version.as_deref(), meta.scope_version())?;
        let mut items = page.items.into_iter().map(movement_row_view).collect::<Vec<_>>();
        let source_document_nos =
            load_movement_source_document_nos(&self.db, self.fulfillment_facts.as_ref(), &items).await?;
        for item in &mut items {
            item.source_document_no = source_document_nos.get(&item.source_document_id).cloned();
        }
        Ok(InventoryListPage::from_page(
            PageView { items, total: page.total, page: query.paging.page, page_size: query.paging.page_size },
            meta,
            authorization.movement_list_scope().is_empty(),
            MOVEMENT_SCOPE_SUMMARY,
            MOVEMENT_OWNERSHIP_BASIS,
        ))
    }

    async fn search_stock_movements(
        &self,
        query: &StockMovementListQuery,
        actor: &AuditActor,
    ) -> Result<(persistence_core::PageResult<StockMovementRow>, InventoryAuthorization)> {
        let db = self.db.clone();
        let authorization_port = std::sync::Arc::clone(&self.authorization);
        let catalog = std::sync::Arc::clone(&self.catalog_facts);
        let query = query.clone();
        let actor = actor.clone();
        self.db
            .client()
            .clone()
            .with_transaction(move |session| {
                Box::pin(async move {
                    let authorization = authorization_port.authorize(&actor, session).await?;
                    if !authorization.actor_is_active() {
                        return Err(Error::Forbidden("当前账号无库存流水读取权限".to_string()));
                    }
                    let search = super::search::sku_filter(
                        catalog.as_ref(),
                        query.q.as_deref(),
                        query.sku_id.as_ref(),
                        session,
                    )
                    .await?;
                    let filter = movement_filter(&query, &authorization, search);
                    let page = db.stock_movements().search_stock_movements(&filter, session).await?;
                    Ok::<_, Error>((page, authorization))
                })
            })
            .await
    }
}

fn movement_filter(
    query: &StockMovementListQuery,
    authorization: &InventoryAuthorization,
    search: crate::repository::InventorySearch,
) -> StockMovementFilter {
    let (sort_by, sort_ascending) = query.paging.sort_selection();
    StockMovementFilter {
        search,
        warehouse_ids: authorization
            .movement_list_scope()
            .repository_warehouse_ids(query.warehouse_id.clone()),
        sku_id: query.sku_id.clone(),
        movement_type: query.movement_type,
        direction: query.direction,
        occurred_from: query.occurred_from.map(Instant::from_unix_secs),
        occurred_to: query.occurred_to.map(Instant::from_unix_secs),
        recorded_by_ids: query.operator_user_ids.clone(),
        page: query.paging.page,
        page_size: query.paging.page_size,
        sort_by,
        sort_ascending,
    }
}

pub(super) fn movement_row_view(row: StockMovementRow) -> StockMovementView {
    StockMovementView {
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
/// * `db` - 数据库实例
/// * `movements` - 流水视图列表
///
/// # 返回
/// 返回「来源单据主键 → 单据号」映射。
///
/// # 错误
/// 查询失败时返回 `RepositoryError`。
pub(super) async fn load_movement_source_document_nos(
    db: &Database,
    fulfillment: &dyn crate::ports::FulfillmentFactsPort,
    movements: &[StockMovementView],
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
        let adjustments =
            db.inventory().stock_adjustments_by_ids(&adjustment_ids, &mut NoTransaction).await?;
        for adjustment in adjustments {
            document_nos.insert(adjustment.base.id.clone(), adjustment.adjustment_no.clone());
        }
    }
    let receipt_ids: Vec<String> = movements
        .iter()
        .filter(|movement| matches!(movement.movement_type, MovementType::PurchaseReceiptIn))
        .map(|movement| movement.source_document_id.clone())
        .collect();
    if !receipt_ids.is_empty() {
        let receipts = fulfillment.receipt_nos_by_ids(&receipt_ids, &mut NoTransaction).await?;
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
