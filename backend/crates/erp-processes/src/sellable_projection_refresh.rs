//! 公司商品池预算投影刷新：按源数据变化、换日与首次构建重算，列表读取前追平少量变化。
//!
//! 变化由 SKU、SKU 修订、商品、商品修订、供给与可供状态的 `updated_at` 扫描发现；
//! 资格含日期条件，换日后全部重算。重算复用实时资格聚合，只取待重算的 SKU，
//! 资格口径只有一处。保存时比对标记序号，多实例并发只会重复计算。

use std::collections::HashMap;

use erp_catalog::ports::supply::CatalogSupplyQueryPort;
use erp_catalog::repository::{CatalogExt, SellableSkuProjectionState, SellableSkuRow};
use erp_core::change_scan::ChangeWindow;
use erp_core::common::time::{BusinessDate, Instant};
use erp_supply::repository::{SupplierOfferingExt, supplier_catalog};
use mongodb::Database;
use persistence_core::dirty_marks::DirtyMark;
use persistence_core::{NoTransaction, Result};

use crate::adapters::catalog_supply_query::MongoCatalogSupplyQuery;

/// 每批重算的 SKU 数。
const RECOMPUTE_BATCH: i64 = 200;
/// 每轮后台最多重算的批数，避免长时间占用统一后台执行器。
const MAX_BATCHES_PER_RUN: usize = 10;
/// 读取前最多同步追平的待重算 SKU 数；更多时先返回现有投影，由后台继续。
const CATCH_UP_LIMIT: usize = 200;

/// 公司商品池预算投影刷新。
pub struct SellableProjectionRefresher {
    db: Database,
}

impl SellableProjectionRefresher {
    /// 绑定数据库。
    ///
    /// # 参数
    /// * `db` - 商品、供给与投影所在数据库
    ///
    /// # 返回
    /// 返回未执行 I/O 的刷新器。
    ///
    /// # 错误
    /// 无。
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// 执行一轮后台刷新：必要时全量重建，换日全部标记，扫描变化，再分批重算。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回本轮保存的投影数。
    ///
    /// # 错误
    /// 读取或写入失败时返回错误；已完成的标记与保存保留，下一轮继续。
    pub async fn run_due(&self) -> Result<usize> {
        supplier_catalog::expire_availability(&self.db, Instant::now(), &mut NoTransaction).await?;
        let (now, today) = (Instant::now().unix_secs(), BusinessDate::today());
        let store = self.db.sellable_sku_projections();
        let state = self.prepare_state(now, today).await?;
        if state.needs_daily_refresh(today) {
            store.mark_all(&mut NoTransaction).await?;
            store.advance(state.watermarks, Some(today), &mut NoTransaction).await?;
        }
        self.mark_changes(state.watermarks.scan_windows(now)).await?;
        store.advance(state.watermarks.advanced(now), None, &mut NoTransaction).await?;
        let saved = self.recompute_batches(now, today).await?;
        if !state.ready && store.dirty_count(&mut NoTransaction).await? == 0 {
            store.mark_ready(state.version, &mut NoTransaction).await?;
            tracing::info!("公司商品池预算投影已就绪");
        }
        Ok(saved)
    }

    /// 列表读取前追平：扫描最新变化，待重算不多时同步重算。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 投影已就绪、可用于列表时为真；尚未就绪时为假，调用方改走实时聚合。
    ///
    /// # 错误
    /// 读取或写入失败时返回错误。
    pub async fn catch_up(&self) -> Result<bool> {
        let store = self.db.sellable_sku_projections();
        let Some(state) = store.state(&mut NoTransaction).await? else {
            return Ok(false);
        };
        if !state.serves_list() {
            return Ok(false);
        }
        let now = Instant::now().unix_secs();
        self.mark_changes(vec![state.watermarks.catch_up_window(now)]).await?;
        store.advance(state.watermarks.fresh_advanced(now), None, &mut NoTransaction).await?;
        let pending = store.dirty_batch(CATCH_UP_LIMIT as i64 + 1, &mut NoTransaction).await?;
        if !pending.is_empty() && pending.len() <= CATCH_UP_LIMIT {
            self.recompute(pending, now, BusinessDate::today()).await?;
        }
        Ok(true)
    }

    /// 读取进度；首次运行或口径升级时把全部 SKU 标记待重算。
    async fn prepare_state(&self, now: i64, today: BusinessDate) -> Result<SellableSkuProjectionState> {
        let store = self.db.sellable_sku_projections();
        let state = store.state(&mut NoTransaction).await?;
        if let Some(state) = state.filter(|state| !SellableSkuProjectionState::needs_rebuild(Some(state))) {
            return Ok(state);
        }
        let sku_ids = store.all_sku_ids(&mut NoTransaction).await?;
        store.mark_skus(&sku_ids, &mut NoTransaction).await?;
        let state = SellableSkuProjectionState::rebuilt(now, today);
        store.begin_rebuild(&state, &mut NoTransaction).await?;
        tracing::info!(sku_count = sku_ids.len(), "公司商品池预算投影开始全量构建");
        Ok(state)
    }

    /// 标记时间窗内商品或供给有变化的 SKU。
    async fn mark_changes(&self, windows: Vec<ChangeWindow>) -> Result<()> {
        let store = self.db.sellable_sku_projections();
        for window in windows {
            let mut sku_ids = store.changed_sku_ids(window, &mut NoTransaction).await?;
            sku_ids
                .extend(self.db.sku_delivery_coverages().changed_sku_ids(window, &mut NoTransaction).await?);
            store.mark_skus(&sku_ids, &mut NoTransaction).await?;
        }
        Ok(())
    }

    /// 分批重算待重算 SKU。
    async fn recompute_batches(&self, now: i64, today: BusinessDate) -> Result<usize> {
        let store = self.db.sellable_sku_projections();
        let mut saved = 0;
        for _ in 0..MAX_BATCHES_PER_RUN {
            let batch = store.dirty_batch(RECOMPUTE_BATCH, &mut NoTransaction).await?;
            let full = batch.len() as i64 == RECOMPUTE_BATCH;
            saved += self.recompute(batch, now, today).await?;
            if !full {
                break;
            }
        }
        Ok(saved)
    }

    /// 按实时资格聚合重算一批 SKU 并保存；不可售的 SKU 保存为空行。
    async fn recompute(&self, marks: Vec<DirtyMark>, now: i64, today: BusinessDate) -> Result<usize> {
        if marks.is_empty() {
            return Ok(0);
        }
        let sku_ids = marks.iter().map(|mark| mark.id.clone()).collect::<Vec<_>>();
        let query = MongoCatalogSupplyQuery::new(self.db.clone());
        let rows = query.find_sellable_skus_by_ids(&sku_ids, today, &mut NoTransaction).await?;
        let rows = rows
            .into_iter()
            .map(|row| (row.sku_id.clone(), row))
            .collect::<HashMap<String, SellableSkuRow>>();
        let store = self.db.sellable_sku_projections();
        let mut saved = 0;
        for mark in marks {
            let row = rows.get(&mark.id);
            if store.save(&mark.id, mark.dirty_token, row, now, &mut NoTransaction).await? {
                saved += 1;
            }
        }
        Ok(saved)
    }
}
