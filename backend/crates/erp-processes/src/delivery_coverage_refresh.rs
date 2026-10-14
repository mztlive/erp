//! SKU 配送覆盖后台刷新：公司商品池地区筛选的预算结果随供给变化重算。
//!
//! 变化由供给、可供状态与配送模板的 `updated_at` 扫描发现：即时扫描处理最新时间窗，
//! 延迟复扫在提交余量后再处理一次同一时间窗，补上开始早、提交晚的事务；按日期生效
//! 或失效由到期日触发；首次运行或口径升级时全量重建。标记与重算均可重入，多实例
//! 并发运行只会重复计算，不会留下过期结果。

use std::collections::HashMap;

use erp_core::common::time::{BusinessDate, Instant};
use erp_read_models::catalog_center::delivery_coverage::sku_delivery_inputs;
use erp_supply::entity::supplier_offering::{SkuDeliveryCoverageMemo, SkuDeliveryCoverageState};
use erp_supply::repository::SupplierOfferingExt;
use mongodb::Database;
use persistence_core::NoTransaction;

use crate::Result;

/// 每批重算的 SKU 数。
const RECOMPUTE_BATCH: i64 = 200;
/// 每轮最多重算的批数，避免长时间占用统一后台执行器。
const MAX_BATCHES_PER_RUN: usize = 5;

/// SKU 配送覆盖刷新流程。
pub struct DeliveryCoverageRefreshProcess {
    db: Database,
}

impl DeliveryCoverageRefreshProcess {
    /// 绑定数据库。
    ///
    /// # 参数
    /// * `db` - 供给、商品与覆盖所在数据库
    ///
    /// # 返回
    /// 返回未执行 I/O 的流程。
    ///
    /// # 错误
    /// 无。
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// 执行一轮刷新：必要时全量重建，扫描变化与到期日标记待重算，再重算一部分。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回本轮保存的覆盖数。
    ///
    /// # 错误
    /// 读取或写入失败时返回错误；已完成的标记与保存保留，下一轮继续。
    pub async fn run_due(&self) -> Result<usize> {
        let now = Instant::now().unix_secs();
        let state = self.prepare_state(now).await?;
        self.mark_changes(&state, now).await?;
        self.db.sku_delivery_coverages().mark_due(BusinessDate::today(), &mut NoTransaction).await?;
        self.recompute(now).await
    }

    /// 读取进度；首次运行或口径升级时把全部有供给的 SKU 标记待重算。
    async fn prepare_state(&self, now: i64) -> Result<SkuDeliveryCoverageState> {
        let store = self.db.sku_delivery_coverages();
        let state = store.state(&mut NoTransaction).await?;
        if let Some(state) = state.filter(|state| !SkuDeliveryCoverageState::needs_rebuild(Some(state))) {
            return Ok(state);
        }
        let sku_ids = store.offered_sku_ids(&mut NoTransaction).await?;
        store.mark_skus(&sku_ids, &mut NoTransaction).await?;
        let state = SkuDeliveryCoverageState::rebuilt(now);
        store.advance_state(&state, &mut NoTransaction).await?;
        tracing::info!(sku_count = sku_ids.len(), "SKU 配送覆盖开始全量重建");
        Ok(state)
    }

    /// 扫描即时与延迟时间窗内的变化并标记受影响 SKU，完成后推进进度。
    async fn mark_changes(&self, state: &SkuDeliveryCoverageState, now: i64) -> Result<()> {
        let store = self.db.sku_delivery_coverages();
        for window in state.watermarks.scan_windows(now) {
            let sku_ids = store.changed_sku_ids(window, &mut NoTransaction).await?;
            store.mark_skus(&sku_ids, &mut NoTransaction).await?;
            let supplier_ids = store.changed_template_supplier_ids(window, &mut NoTransaction).await?;
            store.mark_suppliers(&supplier_ids, &mut NoTransaction).await?;
        }
        store.advance_state(&state.advanced(now), &mut NoTransaction).await?;
        Ok(())
    }

    /// 分批重算待重算 SKU；同一批内相同政策组合只计算一次。
    async fn recompute(&self, now: i64) -> Result<usize> {
        let store = self.db.sku_delivery_coverages();
        let mut memo = SkuDeliveryCoverageMemo::default();
        let mut saved = 0;
        for _ in 0..MAX_BATCHES_PER_RUN {
            let batch = store.dirty_batch(RECOMPUTE_BATCH, &mut NoTransaction).await?;
            let tokens =
                batch.iter().map(|dirty| (dirty.id.clone(), dirty.dirty_token)).collect::<HashMap<_, _>>();
            let sku_ids = batch.into_iter().map(|dirty| dirty.id).collect::<Vec<_>>();
            for input in sku_delivery_inputs(&self.db, &sku_ids, BusinessDate::today()).await? {
                let token = tokens[&input.sku_id];
                let coverage = memo.coverage(&input);
                if store.save(&input.into_record(coverage, now), token, &mut NoTransaction).await? {
                    saved += 1;
                }
            }
            if (sku_ids.len() as i64) < RECOMPUTE_BATCH {
                break;
            }
        }
        Ok(saved)
    }
}
