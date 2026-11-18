use std::collections::{BTreeSet, HashMap, HashSet};

use application_core::AuditActor;
use erp_core::money::Quantity;
use mongodb::Database;
use persistence_core::{Executor, Transactional};
use validator::Validate;

use super::InventoryService;
use super::movement::load_movement_source_document_nos;
use crate::dto::scope::{BALANCE_OWNERSHIP_BASIS, BALANCE_SCOPE_SUMMARY};
use crate::dto::{
    InventoryListPage, PageView, StockBalanceDetailView, StockBalanceListParams, StockBalanceListQuery,
    StockBalanceView, StockMovementView, StockReservationView, ensure_scope_version,
};
use crate::entity::inventory::{StockBalance, StockMovement};
use crate::error::{Error, Result};
use crate::ports::{
    AuthorizationPort, CatalogFactsPort, FulfillmentFactsPort, InventoryAction, InventoryAuthorization,
    InventoryScopeMeta, SkuFact, SkuRevisionFact, WarehouseFact, WarehouseFactsPort, WarehouseRevisionFact,
};
use crate::repository::prelude::*;
use crate::repository::{InventoryExt, StockBalanceFilter, StockBalanceRow, StockMovementFilter};

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
    /// 分页、排序或 `scope_version` 非法时返回 `ValidationError`。
    /// 跨页范围版本不一致时返回 `ConflictError`。
    /// 账号未激活时返回 `Forbidden`。
    /// 授权、商品事实或仓储查询失败时返回对应错误。
    #[tracing::instrument(
        name = "inventory.stock_balance_list",
        skip_all,
        fields(layer = "service", domain = "inventory", operation = "stock_balance_list")
    )]
    pub async fn stock_balance_list(
        &self,
        params: &StockBalanceListParams,
        actor: &AuditActor,
    ) -> Result<InventoryListPage<StockBalanceView>> {
        params.validate()?;
        let query = params.normalized()?;
        let page_no = query.paging.page;
        let page_size = query.paging.page_size;
        let scope_version = query.scope_version.clone();
        let BalancePage {
            page,
            scope_meta,
            no_scope,
            enrichments,
            active_reservation_dims,
            create_warehouses,
        } = self.balance_page(query, actor).await?;
        let items = page
            .items
            .into_iter()
            .map(|row| {
                let balance = StockBalanceRef::from(row);
                let has_active_reservation =
                    active_reservation_dims.contains(&(balance.warehouse_id.clone(), balance.sku_id.clone()));
                let can_create_adjustment = create_warehouses.contains(&balance.warehouse_id);
                enrich_balance_view(balance, &enrichments, has_active_reservation, can_create_adjustment)
            })
            .collect();
        ensure_scope_version(page_no, scope_version.as_deref(), scope_meta.scope_version())?;
        Ok(InventoryListPage::from_page(
            PageView { items, total: page.total, page: page_no, page_size },
            &scope_meta,
            no_scope,
            BALANCE_SCOPE_SUMMARY,
            BALANCE_OWNERSHIP_BASIS,
        ))
    }

    /// 在同一快照依次完成主列表读取和独立按钮资格。
    async fn balance_page(&self, query: StockBalanceListQuery, actor: &AuditActor) -> Result<BalancePage> {
        let service = self.clone();
        let actor = actor.clone();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move { service.read_balance_page(query, &actor, executor).await })
            })
            .await
    }

    /// 主查询补齐完成后关闭读取阶段，再开始当前页创建按钮检查。
    async fn read_balance_page(
        &self,
        query: StockBalanceListQuery,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<BalancePage> {
        let authorization =
            self.authorization.authorize(actor, InventoryAction::BalanceList, executor).await?;
        if !authorization.actor_is_active() {
            return Err(Error::Forbidden("当前账号无库存读取权限".into()));
        }
        let page =
            search_balance_rows(&self.db, self.catalog_facts.as_ref(), &authorization, query, executor)
                .await?;
        let (enrichments, active_reservation_dims) = balance_page_enrichments(
            &self.db,
            self.warehouse_facts.as_ref(),
            self.catalog_facts.as_ref(),
            &page.items,
            executor,
        )
        .await?;
        let scope_meta = authorization.meta(InventoryAction::BalanceList).clone();
        let no_scope = authorization.no_scope(InventoryAction::BalanceList);
        let candidates = page.items.iter().map(|row| row.warehouse_id.to_string()).collect::<Vec<_>>();
        let create_warehouses =
            finish_balance_page(&authorization, self.authorization.as_ref(), actor, &candidates, executor)
                .await?;
        Ok(BalancePage {
            page,
            scope_meta,
            no_scope,
            enrichments,
            active_reservation_dims,
            create_warehouses,
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
    /// 余额不存在、账号未激活或仓库不在详情范围时返回 `NotFound`。
    /// 授权、展示事实或仓储查询失败时返回对应错误。
    #[tracing::instrument(
        name = "inventory.stock_balance_detail",
        skip_all,
        fields(layer = "service", domain = "inventory", operation = "stock_balance_detail")
    )]
    pub async fn stock_balance_detail(&self, id: &str, actor: &AuditActor) -> Result<StockBalanceDetailView> {
        let service = self.clone();
        let actor = actor.clone();
        let id = id.to_owned();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move { service.read_balance_detail(&id, &actor, executor).await })
            })
            .await
    }

    /// 主余额阶段交付后，独立核对附属调整读取和创建按钮资格。
    async fn read_balance_detail(
        &self,
        id: &str,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<StockBalanceDetailView> {
        let mut read = self.balance_detail_core(id, actor, executor).await?;
        if super::warehouse_allowed(
            self.authorization.as_ref(),
            actor,
            InventoryAction::AdjustmentDetail,
            read.balance.warehouse_id.as_ref(),
            executor,
        )
        .await?
        {
            read.view.pending_adjustments = self
                .db
                .inventory()
                .pending_adjustments_for_warehouse(&read.balance.warehouse_id, executor)
                .await?
                .into_iter()
                .map(Into::into)
                .collect();
        }
        let can_create = super::warehouse_allowed(
            self.authorization.as_ref(),
            actor,
            InventoryAction::Create,
            read.balance.warehouse_id.as_ref(),
            executor,
        )
        .await?;
        read.view.balance.allowed_actions = balance_allowed_actions(can_create && read.has_warehouse);
        Ok(read.view)
    }

    /// 完成余额、最近流水、预占和展示补齐的唯一主查询阶段。
    async fn balance_detail_core(
        &self,
        id: &str,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<BalanceDetailRead> {
        let authorization =
            self.authorization.authorize(actor, InventoryAction::BalanceDetail, executor).await?;
        if !authorization.actor_is_active() {
            return Err(Error::NotFound("库存余额不存在".into()));
        }
        let balance = self
            .db
            .inventory()
            .authorized_stock_balance(id, authorization.query(InventoryAction::BalanceDetail), executor)
            .await?
            .ok_or_else(|| Error::NotFound("库存余额不存在".into()))?;
        let details = balance_details(&self.db, self.fulfillment_facts.as_ref(), &balance, executor).await?;
        let (view, has_warehouse) = self.enriched_balance_detail(&balance, details, executor).await?;
        authorization.validate(executor).await?;
        Ok(BalanceDetailRead { balance, view, has_warehouse })
    }

    /// 展示补齐只在主查询阶段内读取；独立动作不得复用此阶段凭证。
    async fn enriched_balance_detail(
        &self,
        balance: &StockBalance,
        details: BalanceDetails,
        executor: &mut dyn Executor,
    ) -> Result<(StockBalanceDetailView, bool)> {
        let enrichments = load_balance_enrichments(
            &self.db,
            self.warehouse_facts.as_ref(),
            self.catalog_facts.as_ref(),
            &[balance.warehouse_id.to_string()],
            &[balance.sku_id.to_string()],
            &[],
            executor,
        )
        .await?;
        let has_warehouse = enrichments.warehouses.contains_key(balance.warehouse_id.as_ref());
        let balance_view =
            enrich_balance_view(balance.into(), &enrichments, !details.reservations.is_empty(), false);
        Ok((
            StockBalanceDetailView {
                balance: balance_view,
                recent_movements: details.movements,
                active_reservations: details.reservations,
                pending_adjustments: Vec::new(),
            },
            has_warehouse,
        ))
    }
}

/// 主结果已经完成补齐，关闭凭证后才允许进入独立按钮阶段。
async fn finish_balance_page(
    authorization: &InventoryAuthorization,
    port: &dyn AuthorizationPort,
    actor: &AuditActor,
    candidates: &[String],
    executor: &mut dyn Executor,
) -> Result<BTreeSet<String>> {
    authorization.validate(executor).await?;
    port.allowed_warehouses(actor, InventoryAction::Create, candidates, executor).await
}

struct BalanceDetailRead {
    balance: StockBalance,
    view: StockBalanceDetailView,
    has_warehouse: bool,
}

struct BalancePage {
    page: persistence_core::PageResult<StockBalanceRow>,
    scope_meta: InventoryScopeMeta,
    no_scope: bool,
    enrichments: BalanceEnrichments,
    active_reservation_dims: HashSet<(String, String)>,
    create_warehouses: BTreeSet<String>,
}

async fn search_balance_rows(
    db: &Database,
    catalog: &dyn CatalogFactsPort,
    authorization: &InventoryAuthorization,
    query: StockBalanceListQuery,
    executor: &mut dyn Executor,
) -> Result<persistence_core::PageResult<StockBalanceRow>> {
    let search =
        super::search::sku_filter(catalog, query.q.as_deref(), query.sku_id.as_ref(), executor).await?;
    let search = db
        .inventory()
        .balance_search(search, query.balance_id.as_deref(), query.availability, executor)
        .await?;
    let (sort_by, sort_ascending) = query.paging.sort_selection();
    let filter = StockBalanceFilter {
        search,
        warehouse_ids: query.warehouse_id.map(|id| vec![id]),
        sku_id: query.sku_id,
        page: query.paging.page,
        page_size: query.paging.page_size,
        sort_by,
        sort_ascending,
    };
    db.stock_balances()
        .search_authorized_stock_balances(
            &filter,
            authorization.query(InventoryAction::BalanceList),
            executor,
        )
        .await
}

async fn load_balance_enrichments(
    db: &Database,
    warehouse: &dyn WarehouseFactsPort,
    catalog: &dyn CatalogFactsPort,
    warehouse_ids: &[String],
    sku_ids: &[String],
    movement_ids: &[String],
    executor: &mut dyn Executor,
) -> Result<BalanceEnrichments> {
    let warehouses = warehouse.warehouses_by_ids(warehouse_ids, executor).await?;
    let skus = catalog.skus_by_ids(sku_ids, executor).await?;
    let warehouse_revision_ids: Vec<String> =
        warehouses.values().filter_map(|warehouse| warehouse.current_revision_id.clone()).collect();
    let warehouse_revisions = warehouse.warehouse_revisions_by_ids(&warehouse_revision_ids, executor).await?;
    let sku_revision_ids: Vec<String> =
        skus.values().filter_map(|sku| sku.current_revision_id.clone()).collect();
    let sku_revisions = catalog.sku_revisions_by_ids(&sku_revision_ids, executor).await?;
    let movements = load_movements_by_ids(db, movement_ids, executor).await?;
    Ok(BalanceEnrichments { warehouses, warehouse_revisions, skus, sku_revisions, movements })
}

async fn balance_page_enrichments(
    db: &Database,
    warehouse: &dyn WarehouseFactsPort,
    catalog: &dyn CatalogFactsPort,
    rows: &[StockBalanceRow],
    executor: &mut dyn Executor,
) -> Result<(BalanceEnrichments, HashSet<(String, String)>)> {
    let warehouse_ids = rows.iter().map(|row| row.warehouse_id.to_string()).collect::<Vec<_>>();
    let sku_ids = rows.iter().map(|row| row.sku_id.to_string()).collect::<Vec<_>>();
    let movement_ids = rows
        .iter()
        .filter_map(|row| row.last_movement_id.as_ref().map(ToString::to_string))
        .collect::<Vec<_>>();
    let enriched =
        load_balance_enrichments(db, warehouse, catalog, &warehouse_ids, &sku_ids, &movement_ids, executor)
            .await?;
    let active = active_reservation_dims(db, &warehouse_ids, &sku_ids, executor).await?;
    Ok((enriched, active))
}

/// 余额详情的受限附属业务事实，全部在主读取事务内取证。
struct BalanceDetails {
    movements: Vec<StockMovementView>,
    reservations: Vec<StockReservationView>,
}

async fn balance_details(
    db: &Database,
    fulfillment: &dyn FulfillmentFactsPort,
    balance: &StockBalance,
    executor: &mut dyn Executor,
) -> Result<BalanceDetails> {
    let filter = StockMovementFilter {
        warehouse_ids: Some(vec![balance.warehouse_id.clone()]),
        sku_id: Some(balance.sku_id.clone()),
        page_size: 8,
        sort_by: Some("occurred_at".into()),
        ..Default::default()
    };
    let page = db.stock_movements().search_stock_movements(&filter, executor).await?;
    let reservations = db
        .inventory()
        .operable_reservations_for_balance(&balance.warehouse_id, &balance.sku_id, executor)
        .await?;
    let mut movements = page.items.into_iter().map(super::movement::movement_row_view).collect::<Vec<_>>();
    let numbers = load_movement_source_document_nos(db, fulfillment, &movements, executor).await?;
    for movement in &mut movements {
        movement.source_document_no = numbers.get(&movement.source_document_id).cloned();
    }
    Ok(BalanceDetails { movements, reservations: reservations.into_iter().map(Into::into).collect() })
}

/// 余额列表/详情的基础信息投影映射。
struct BalanceEnrichments {
    /// 仓库按主键索引。
    warehouses: HashMap<String, WarehouseFact>,
    /// 仓库修订按主键索引。
    warehouse_revisions: HashMap<String, WarehouseRevisionFact>,
    /// SKU 按主键索引。
    skus: HashMap<String, SkuFact>,
    /// SKU 修订按主键索引。
    sku_revisions: HashMap<String, SkuRevisionFact>,
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
    executor: &mut dyn Executor,
) -> Result<HashSet<(String, String)>> {
    let reservations =
        db.inventory().operable_reservations_for_dimensions(warehouse_ids, sku_ids, executor).await?;
    Ok(reservations
        .into_iter()
        .map(|reservation| (reservation.warehouse_id.to_string(), reservation.sku_id.to_string()))
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
async fn load_movements_by_ids(
    db: &Database,
    ids: &[String],
    executor: &mut dyn Executor,
) -> Result<HashMap<String, StockMovement>> {
    if ids.is_empty() {
        return Ok(HashMap::new());
    }
    let movements = db.inventory().movements_by_ids(ids, executor).await?;
    Ok(movements.into_iter().map(|movement| (movement.base.id.clone(), movement)).collect())
}

/// 余额视图映射所需的最小余额事实（列表投影行与余额实体共用）。
struct StockBalanceRef {
    /// 余额主键。
    id: String,
    /// 仓库主键。
    warehouse_id: String,
    /// SKU 主键。
    sku_id: String,
    /// 账面现存。
    on_hand_quantity: Quantity,
    /// 有效预占。
    reserved_quantity: Quantity,
    /// 可用数量。
    available_quantity: Quantity,
    /// 乐观锁版本（十进制字符串）。
    version: String,
    /// 已应用最后流水。
    last_movement_id: Option<String>,
}

impl From<StockBalanceRow> for StockBalanceRef {
    /// 由列表投影行组装最小余额事实。
    fn from(row: StockBalanceRow) -> Self {
        Self {
            id: row.id,
            warehouse_id: row.warehouse_id.to_string(),
            sku_id: row.sku_id.to_string(),
            on_hand_quantity: row.on_hand_quantity,
            reserved_quantity: row.reserved_quantity,
            available_quantity: row.available_quantity,
            version: row.version.to_string(),
            last_movement_id: row.last_movement_id.as_ref().map(ToString::to_string),
        }
    }
}

impl From<&StockBalance> for StockBalanceRef {
    /// 由余额实体组装最小余额事实（详情与列表共用同一视图组装）。
    fn from(balance: &StockBalance) -> Self {
        Self {
            id: balance.base.id.clone(),
            warehouse_id: balance.warehouse_id.to_string(),
            sku_id: balance.sku_id.to_string(),
            on_hand_quantity: balance.on_hand_quantity,
            reserved_quantity: balance.reserved_quantity,
            available_quantity: balance.available_quantity,
            version: balance.base.version.to_string(),
            last_movement_id: balance.last_movement_id.as_ref().map(ToString::to_string),
        }
    }
}
/// 由余额事实与基础信息投影组装余额视图（列表与详情共用）。
///
/// # 参数
/// * `balance` - 最小余额事实
/// * `enrichments` - 仓库/SKU/流水基础信息投影
/// * `has_active_reservation` - 是否存在有效预占
/// * `can_create_adjustment` - 调用人是否可在该仓库创建调整
///
/// # 返回
/// 返回契约形状的余额视图。
fn enrich_balance_view(
    balance: StockBalanceRef,
    enrichments: &BalanceEnrichments,
    has_active_reservation: bool,
    can_create_adjustment: bool,
) -> StockBalanceView {
    let warehouse = enrichments.warehouses.get(&balance.warehouse_id);
    let warehouse_name = warehouse
        .and_then(|wh| wh.current_revision_id.as_deref())
        .and_then(|revision_id| enrichments.warehouse_revisions.get(revision_id))
        .map(|revision| revision.name.clone());
    let sku = enrichments.skus.get(&balance.sku_id);
    let sku_revision = sku
        .and_then(|sku| sku.current_revision_id.as_deref())
        .and_then(|revision_id| enrichments.sku_revisions.get(revision_id));
    let last_movement = balance.last_movement_id.as_ref().and_then(|id| enrichments.movements.get(id));
    StockBalanceView {
        id: balance.id,
        warehouse_id: balance.warehouse_id,
        warehouse_code: warehouse.map(|wh| wh.warehouse_code.clone()).unwrap_or_default(),
        warehouse_name: warehouse_name.unwrap_or_default(),
        sku_id: balance.sku_id,
        sku_code: sku.map(|sku| sku.sku_no.clone()).unwrap_or_default(),
        sku_name: sku_revision.map(|revision| revision.name.clone()).unwrap_or_default(),
        spec_summary: sku_revision.and_then(|revision| revision.specification.clone()),
        on_hand_quantity: balance.on_hand_quantity,
        reserved_quantity: balance.reserved_quantity,
        available_quantity: balance.available_quantity,
        version: balance.version,
        last_movement_id: balance.last_movement_id,
        last_movement_at: last_movement.map(|movement| movement.fact.occurred_at.unix_secs()),
        last_movement_type: last_movement.map(|movement| movement.movement_type),
        has_active_reservation,
        allowed_actions: balance_allowed_actions(can_create_adjustment && warehouse.is_some()),
    }
}

fn balance_allowed_actions(can_create_adjustment: bool) -> Vec<String> {
    can_create_adjustment.then(|| "CREATE_ADJUSTMENT".to_string()).into_iter().collect()
}

#[cfg(test)]
mod tests;
