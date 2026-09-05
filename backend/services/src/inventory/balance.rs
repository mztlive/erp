use std::collections::{HashMap, HashSet};

use database::{InventoryExt, NoTransaction, Transactional};
use entities::inventory::{StockBalance, StockMovement};
use mongodb::Database;
use validator::Validate;

use super::authorization::inventory_authorization_with_executor;
use super::dto::{
    PageView, SortDir, StockBalanceDetailView, StockBalanceListParams, StockBalanceView, StockMovementView,
};
use super::movement::load_movement_source_document_nos;
use super::InventoryService;
use crate::audit::AuditActor;
use crate::errors::{Error, Result};

/// 库存余额列表筛选条件类型（经 `InventoryExt` 关联类型跨 crate 可达）。
type StockBalanceFilter = <mongodb::Database as InventoryExt>::StockBalanceFilter;
/// 库存流水筛选条件类型（经 `InventoryExt` 关联类型跨 crate 可达）。
type StockMovementFilter = <mongodb::Database as InventoryExt>::StockMovementFilter;

impl InventoryService {
    /// 分页查询库存余额列表（W10 余额视图）。
    ///
    /// 列表行按页批量投影仓库与 SKU 基础信息（D11/D10 跨域只读），
    /// 不做逐行查询（禁止 N+1）。
    ///
    /// # 参数
    /// * `params` - 查询参数（`warehouse_id`/`sku_id` 扁平筛选）
    /// * `actor` - 当前认证操作人，用于计算余额范围与调整动作
    ///
    /// # 返回
    /// 返回契约形状的分页视图（`items`/`total`/`page`/`page_size`）。
    ///
    /// # 错误
    /// * `ValidationError` - 分页参数非法或排序字段不在白名单
    /// * `RepositoryError` - 数据库查询失败
    #[tracing::instrument(
        name = "inventory.stock_balance_list",
        skip_all,
        fields(layer = "service", domain = "inventory", operation = "stock_balance_list")
    )]
    pub async fn stock_balance_list(
        &self,
        params: &StockBalanceListParams,
        actor: &AuditActor,
    ) -> Result<PageView<StockBalanceView>> {
        params.validate()?;
        let query = params.normalized()?;
        let page_no = query.paging.page;
        let page_size = query.paging.page_size;
        let db = self.db.clone();
        let rbac = self.rbac.clone();
        let actor = actor.clone();
        let client = db.client().clone();
        let (page, authorization) = client
            .with_transaction(move |session| {
                Box::pin(async move {
                    let authorization =
                        inventory_authorization_with_executor(&db, &rbac, &actor, session).await?;
                    if !authorization.actor_is_active() {
                        return Err(Error::Forbidden("当前账号无库存读取权限".to_string()));
                    }
                    let filter = StockBalanceFilter {
                        warehouse_ids: authorization
                            .balance_list_scope()
                            .repository_warehouse_ids(query.warehouse_id),
                        sku_id: query.sku_id,
                        page: query.paging.page,
                        page_size: query.paging.page_size,
                        sort_by: Some(query.paging.sort_by.to_string()),
                        sort_ascending: matches!(query.paging.sort_dir, SortDir::Asc),
                    };
                    let page = db
                        .stock_balances()
                        .search_stock_balances(&filter, session)
                        .await?;
                    Ok::<_, Error>((page, authorization))
                })
            })
            .await?;
        let warehouse_ids: Vec<String> = page
            .items
            .iter()
            .map(|row| row.warehouse_id.to_string())
            .collect();
        let sku_ids: Vec<String> = page.items.iter().map(|row| row.sku_id.to_string()).collect();
        let movement_ids: Vec<String> = page
            .items
            .iter()
            .filter_map(|row| row.last_movement_id.as_ref().map(ToString::to_string))
            .collect();
        let enrichments = self
            .load_enrichments(&warehouse_ids, &sku_ids, &movement_ids)
            .await?;
        let active_reservation_dims = active_reservation_dims(&self.db, &warehouse_ids, &sku_ids).await?;
        let items = page
            .items
            .into_iter()
            .map(|row| {
                let warehouse = enrichments.warehouses.get(&row.warehouse_id.to_string());
                let warehouse_name = warehouse
                    .and_then(|wh| wh.stable.current_revision_id.as_deref())
                    .and_then(|revision_id| enrichments.warehouse_revisions.get(revision_id))
                    .map(|revision| revision.name.clone());
                let sku = enrichments.skus.get(&row.sku_id.to_string());
                let sku_revision = sku
                    .and_then(|sku| sku.stable.current_revision_id.as_deref())
                    .and_then(|revision_id| enrichments.sku_revisions.get(revision_id));
                StockBalanceView {
                    id: row.id,
                    warehouse_id: row.warehouse_id.to_string(),
                    warehouse_code: warehouse.map(|wh| wh.warehouse_code.clone()).unwrap_or_default(),
                    warehouse_name: warehouse_name.unwrap_or_default(),
                    sku_id: row.sku_id.to_string(),
                    sku_code: sku.map(|sku| sku.sku_no.clone()).unwrap_or_default(),
                    sku_name: sku_revision
                        .map(|revision| revision.name.clone())
                        .unwrap_or_default(),
                    spec_summary: sku_revision.and_then(|revision| revision.specification.clone()),
                    on_hand_quantity: row.on_hand_quantity,
                    reserved_quantity: row.reserved_quantity,
                    available_quantity: row.available_quantity,
                    version: row.version.to_string(),
                    last_movement_id: row.last_movement_id.as_ref().map(ToString::to_string),
                    last_movement_at: row
                        .last_movement_id
                        .as_ref()
                        .and_then(|id| enrichments.movements.get(&id.to_string()))
                        .map(|movement| movement.fact.occurred_at.unix_secs()),
                    last_movement_type: row
                        .last_movement_id
                        .as_ref()
                        .and_then(|id| enrichments.movements.get(&id.to_string()))
                        .map(|movement| movement.movement_type),
                    has_active_reservation: active_reservation_dims
                        .contains(&(row.warehouse_id.to_string(), row.sku_id.to_string())),
                    allowed_actions: balance_allowed_actions(
                        warehouse.is_some() && authorization.can_create(row.warehouse_id.as_ref()),
                    ),
                }
            })
            .collect();

        Ok(PageView {
            items,
            total: page.total,
            page: page_no,
            page_size,
        })
    }

    /// 查询库存余额详情（W10 详情：余额 + 最近流水 + 有效预占 + 未过账调整）。
    ///
    /// # 参数
    /// * `id` - 余额主键
    /// * `actor` - 当前认证操作人，用于验证余额范围与调整动作
    ///
    /// # 返回
    /// 返回余额详情视图。
    ///
    /// # 错误
    /// * `NotFound` - 余额不存在
    /// * `RepositoryError` - 数据库查询失败
    #[tracing::instrument(
        name = "inventory.stock_balance_detail",
        skip_all,
        fields(layer = "service", domain = "inventory", operation = "stock_balance_detail")
    )]
    pub async fn stock_balance_detail(&self, id: &str, actor: &AuditActor) -> Result<StockBalanceDetailView> {
        let db = self.db.clone();
        let rbac = self.rbac.clone();
        let actor = actor.clone();
        let id = id.to_string();
        let client = db.client().clone();
        let (balance, authorization) = client
            .with_transaction(move |session| {
                Box::pin(async move {
                    let authorization =
                        inventory_authorization_with_executor(&db, &rbac, &actor, session).await?;
                    let balance = db
                        .inventory()
                        .stock_balance(&id, session)
                        .await?
                        .ok_or_else(|| Error::NotFound("库存余额不存在".to_string()))?;
                    if !authorization.actor_is_active()
                        || !authorization.can_read_balance_detail(balance.warehouse_id.as_ref())
                    {
                        return Err(Error::NotFound("库存余额不存在".to_string()));
                    }
                    Ok::<_, Error>((balance, authorization))
                })
            })
            .await?;
        let filter = StockMovementFilter {
            warehouse_ids: Some(vec![balance.warehouse_id.clone()]),
            sku_id: Some(balance.sku_id.clone()),
            movement_type: None,
            direction: None,
            occurred_from: None,
            occurred_to: None,
            page: 1,
            page_size: 8,
            sort_by: Some("occurred_at".to_string()),
            sort_ascending: false,
        };
        let movements = self
            .db
            .stock_movements()
            .search_stock_movements(&filter, &mut NoTransaction)
            .await?;
        let reservations = self
            .db
            .inventory()
            .operable_reservations_for_balance(&balance.warehouse_id, &balance.sku_id, &mut NoTransaction)
            .await?;
        let pending = if authorization.read_scope().covers(balance.warehouse_id.as_ref()) {
            self.db
                .inventory()
                .pending_adjustments_for_warehouse(&balance.warehouse_id, &mut NoTransaction)
                .await?
        } else {
            Vec::new()
        };
        let enrichments = self
            .load_enrichments(
                &[balance.warehouse_id.to_string()],
                &[balance.sku_id.to_string()],
                &[],
            )
            .await?;
        let balance_view = build_balance_view(
            &balance,
            &enrichments,
            !reservations.is_empty(),
            authorization.can_create(balance.warehouse_id.as_ref()),
        );
        let mut recent_movements: Vec<StockMovementView> = movements
            .items
            .into_iter()
            .map(|row| StockMovementView {
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
                recorded_by: row.recorded_by.clone(),
            })
            .collect();
        let source_document_nos = load_movement_source_document_nos(&self.db, &recent_movements).await?;
        for movement in &mut recent_movements {
            movement.source_document_no = source_document_nos.get(&movement.source_document_id).cloned();
        }
        Ok(StockBalanceDetailView {
            balance: balance_view,
            recent_movements,
            active_reservations: reservations.into_iter().map(Into::into).collect(),
            pending_adjustments: pending.into_iter().map(Into::into).collect(),
        })
    }

    /// 批量加载余额列表/详情的基础信息投影（仓库/仓库修订/SKU/SKU修订/最后流水）。
    ///
    /// 全部按 `$in` 批量取回（禁止 N+1）。
    ///
    /// # 参数
    /// * `warehouse_ids` - 仓库主键集合
    /// * `sku_ids` - SKU 主键集合
    /// * `movement_ids` - 流水主键集合（可为空）
    ///
    /// # 返回
    /// 返回按主键索引的投影映射。
    ///
    /// # 错误
    /// 任一批量查询失败时返回 `RepositoryError`。
    async fn load_enrichments(
        &self,
        warehouse_ids: &[String],
        sku_ids: &[String],
        movement_ids: &[String],
    ) -> Result<BalanceEnrichments> {
        let warehouses = load_warehouses_by_ids(&self.db, warehouse_ids).await?;
        let skus = load_skus_by_ids(&self.db, sku_ids).await?;
        let warehouse_revisions = load_warehouse_revisions(&self.db, &warehouses).await?;
        let sku_revisions = load_sku_revisions(&self.db, &skus).await?;
        let movements = load_movements_by_ids(&self.db, movement_ids).await?;
        Ok(BalanceEnrichments {
            warehouses,
            warehouse_revisions,
            skus,
            sku_revisions,
            movements,
        })
    }
}

/// 余额列表/详情的基础信息投影映射。
struct BalanceEnrichments {
    /// 仓库按主键索引。
    warehouses: HashMap<String, entities::warehouse::Warehouse>,
    /// 仓库修订按主键索引。
    warehouse_revisions: HashMap<String, entities::warehouse::WarehouseRevision>,
    /// SKU 按主键索引。
    skus: HashMap<String, entities::catalog::Sku>,
    /// SKU 修订按主键索引。
    sku_revisions: HashMap<String, entities::catalog::SkuRevision>,
    /// 库存流水按主键索引。
    movements: HashMap<String, StockMovement>,
}

/// 批量取页内余额维度上是否存在有效预占（禁止 N+1）。
///
/// # 参数
/// * `db` - 数据库实例
/// * `warehouse_ids` - 页内仓库主键集合
/// * `sku_ids` - 页内 SKU 主键集合
///
/// # 返回
/// 返回「(仓库, SKU)」有效预占维度集合。
///
/// # 错误
/// 批量查询失败时返回 `RepositoryError`。
async fn active_reservation_dims(
    db: &Database,
    warehouse_ids: &[String],
    sku_ids: &[String],
) -> Result<HashSet<(String, String)>> {
    if warehouse_ids.is_empty() || sku_ids.is_empty() {
        return Ok(HashSet::new());
    }
    let reservations = db
        .inventory()
        .operable_reservations_for_dimensions(warehouse_ids, sku_ids, &mut NoTransaction)
        .await?;
    Ok(reservations
        .into_iter()
        .map(|reservation| {
            (
                reservation.warehouse_id.to_string(),
                reservation.sku_id.to_string(),
            )
        })
        .collect())
}

/// 按主键集合批量取回仓库（`$in`，禁止 N+1）。
///
/// # 参数
/// * `db` - 数据库实例
/// * `ids` - 仓库主键集合
///
/// # 返回
/// 返回按主键索引的仓库映射。
///
/// # 错误
/// 查询失败时返回 `RepositoryError`。
async fn load_warehouses_by_ids(
    db: &Database,
    ids: &[String],
) -> Result<HashMap<String, entities::warehouse::Warehouse>> {
    if ids.is_empty() {
        return Ok(HashMap::new());
    }
    let warehouses = db.inventory().warehouses_by_ids(ids, &mut NoTransaction).await?;
    Ok(warehouses
        .into_iter()
        .map(|warehouse| (warehouse.base.id.clone(), warehouse))
        .collect())
}

/// 按主键集合批量取回 SKU（`$in`，禁止 N+1）。
///
/// # 参数
/// * `db` - 数据库实例
/// * `ids` - SKU 主键集合
///
/// # 返回
/// 返回按主键索引的 SKU 映射。
///
/// # 错误
/// 查询失败时返回 `RepositoryError`。
async fn load_skus_by_ids(db: &Database, ids: &[String]) -> Result<HashMap<String, entities::catalog::Sku>> {
    if ids.is_empty() {
        return Ok(HashMap::new());
    }
    let skus = db.inventory().skus_by_ids(ids, &mut NoTransaction).await?;
    Ok(skus.into_iter().map(|sku| (sku.base.id.clone(), sku)).collect())
}

/// 按当前修订主键集合批量取回仓库修订（`$in`，禁止 N+1）。
///
/// # 参数
/// * `db` - 数据库实例
/// * `warehouses` - 仓库映射
///
/// # 返回
/// 返回按主键索引的仓库修订映射。
///
/// # 错误
/// 查询失败时返回 `RepositoryError`。
async fn load_warehouse_revisions(
    db: &Database,
    warehouses: &HashMap<String, entities::warehouse::Warehouse>,
) -> Result<HashMap<String, entities::warehouse::WarehouseRevision>> {
    let revision_ids: Vec<String> = warehouses
        .values()
        .filter_map(|warehouse| warehouse.stable.current_revision_id.clone())
        .collect();
    if revision_ids.is_empty() {
        return Ok(HashMap::new());
    }
    let revisions = db
        .inventory()
        .warehouse_revisions_by_ids(&revision_ids, &mut NoTransaction)
        .await?;
    Ok(revisions
        .into_iter()
        .map(|revision| (revision.base.id.clone(), revision))
        .collect())
}

/// 按当前修订主键集合批量取回 SKU 修订（`$in`，禁止 N+1）。
///
/// # 参数
/// * `db` - 数据库实例
/// * `skus` - SKU 映射
///
/// # 返回
/// 返回按主键索引的 SKU 修订映射。
///
/// # 错误
/// 查询失败时返回 `RepositoryError`。
async fn load_sku_revisions(
    db: &Database,
    skus: &HashMap<String, entities::catalog::Sku>,
) -> Result<HashMap<String, entities::catalog::SkuRevision>> {
    let revision_ids: Vec<String> = skus
        .values()
        .filter_map(|sku| sku.stable.current_revision_id.clone())
        .collect();
    if revision_ids.is_empty() {
        return Ok(HashMap::new());
    }
    let revisions = db
        .inventory()
        .sku_revisions_by_ids(&revision_ids, &mut NoTransaction)
        .await?;
    Ok(revisions
        .into_iter()
        .map(|revision| (revision.base.id.clone(), revision))
        .collect())
}

/// 按主键集合批量取回库存流水（`$in`，禁止 N+1）。
///
/// # 参数
/// * `db` - 数据库实例
/// * `ids` - 流水主键集合
///
/// # 返回
/// 返回按主键索引的流水映射。
///
/// # 错误
/// 查询失败时返回 `RepositoryError`。
async fn load_movements_by_ids(db: &Database, ids: &[String]) -> Result<HashMap<String, StockMovement>> {
    if ids.is_empty() {
        return Ok(HashMap::new());
    }
    let movements = db.inventory().movements_by_ids(ids, &mut NoTransaction).await?;
    Ok(movements
        .into_iter()
        .map(|movement| (movement.base.id.clone(), movement))
        .collect())
}

fn build_balance_view(
    balance: &StockBalance,
    enrichments: &BalanceEnrichments,
    has_active_reservation: bool,
    can_create_adjustment: bool,
) -> StockBalanceView {
    let warehouse = enrichments.warehouses.get(&balance.warehouse_id.to_string());
    let warehouse_name = warehouse
        .and_then(|wh| wh.stable.current_revision_id.as_deref())
        .and_then(|revision_id| enrichments.warehouse_revisions.get(revision_id))
        .map(|revision| revision.name.clone());
    let sku = enrichments.skus.get(&balance.sku_id.to_string());
    let sku_revision = sku
        .and_then(|sku| sku.stable.current_revision_id.as_deref())
        .and_then(|revision_id| enrichments.sku_revisions.get(revision_id));
    StockBalanceView {
        id: balance.base.id.clone(),
        warehouse_id: balance.warehouse_id.to_string(),
        warehouse_code: warehouse.map(|wh| wh.warehouse_code.clone()).unwrap_or_default(),
        warehouse_name: warehouse_name.unwrap_or_default(),
        sku_id: balance.sku_id.to_string(),
        sku_code: sku.map(|sku| sku.sku_no.clone()).unwrap_or_default(),
        sku_name: sku_revision
            .map(|revision| revision.name.clone())
            .unwrap_or_default(),
        spec_summary: sku_revision.and_then(|revision| revision.specification.clone()),
        on_hand_quantity: balance.on_hand_quantity,
        reserved_quantity: balance.reserved_quantity,
        available_quantity: balance.available_quantity,
        version: balance.base.version.to_string(),
        last_movement_id: balance.last_movement_id.as_ref().map(ToString::to_string),
        last_movement_at: balance
            .last_movement_id
            .as_ref()
            .and_then(|id| enrichments.movements.get(&id.to_string()))
            .map(|movement| movement.fact.occurred_at.unix_secs()),
        last_movement_type: balance
            .last_movement_id
            .as_ref()
            .and_then(|id| enrichments.movements.get(&id.to_string()))
            .map(|movement| movement.movement_type),
        has_active_reservation,
        allowed_actions: balance_allowed_actions(can_create_adjustment && warehouse.is_some()),
    }
}

fn balance_allowed_actions(can_create_adjustment: bool) -> Vec<String> {
    can_create_adjustment
        .then(|| "CREATE_ADJUSTMENT".to_string())
        .into_iter()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::balance_allowed_actions;

    #[test]
    fn balance_create_action_is_only_emitted_from_server_authorization() {
        assert!(balance_allowed_actions(false).is_empty());
        assert_eq!(
            balance_allowed_actions(true),
            vec!["CREATE_ADJUSTMENT".to_string()]
        );
    }
}
