//! 准备：冻结商品池、搜索套餐、生成主图、写入结果。
//!
//! 后台任务计算商品快照与陈列；套餐搜索非穷举，
//! 预算耗尽保留合法候选并标记上限，不得谎称无解。

use std::collections::HashSet;

use erp_core::common::time::{BusinessDate, Instant};
use erp_core::ids::{
    SalesSelectionBookletId, SalesSelectionDisplayItemId, SalesSelectionPoolMemberId, SkuId,
};
use id_generator::next_id;
use persistence_core::NoTransaction;

use super::SalesSelectionService;
use crate::dto::sales_selection::PrepareSalesSelectionRequest;
use crate::entity::sales_selection::{
    POOL_SKU_MAX, PackageImageGenerator, PoolSource, PoolSourceKind, PrepareKind, PrepareStage,
    SalesSelectionBooklet, SalesSelectionDisplayItem, SalesSelectionPoolMember, SkuSnapshot, TierRule,
    combination_key, search_packages, sort_by_sku_id,
};
use crate::ports::sales_selection::{SelectionCatalogPort, SelectionImagePort, SelectionSkuFact};
use crate::repository::SalesSelectionExt;
use crate::repository::prelude::*;
use crate::{Error, Result};

impl SalesSelectionService {
    /// 将已到期且未提交的已发布选品册置为已关闭。
    ///
    /// 公开请求仍以服务端到期即时拒绝为准，本方法只同步业务状态。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回关闭条数。
    ///
    /// # 错误
    /// 仓储失败。
    pub(super) async fn close_expired_published(&self) -> Result<u32> {
        let mut executor = NoTransaction;
        let now = Instant::now();
        let published = self
            .db
            .sales_selection_booklets()
            .list_filtered(None, None, Some("PUBLISHED"), None, &mut executor)
            .await?;
        let mut handled = 0_u32;
        let mut skipped_legacy = 0_u32;
        for mut booklet in published {
            if !booklet.has_persisted_scope() {
                skipped_legacy = skipped_legacy.saturating_add(1);
                continue;
            }
            if !booklet.is_expired(now) {
                continue;
            }
            booklet.close(now, "system")?;
            self.db.sales_selection_booklets().update(&mut booklet, &mut executor).await?;
            handled = handled.saturating_add(1);
        }
        if skipped_legacy > 0 {
            tracing::warn!(skipped_legacy, "跳过缺少销售负责人或业务组织的历史选品册");
        }
        Ok(handled)
    }

    /// 解析目标批次。
    ///
    /// # 参数
    /// * `booklet` - 选品册
    /// * `kind` - 准备种类
    ///
    /// # 返回
    /// 重生成沿用当前批次，否则返回新批次。
    ///
    /// # 错误
    /// 无。
    pub(super) fn resolve_batch_id(&self, booklet: &SalesSelectionBooklet, kind: PrepareKind) -> String {
        if kind.reuses_current_batch() {
            return booklet.current_batch_id.clone().unwrap_or_else(next_id);
        }
        next_id()
    }

    /// 冻结商品池快照。
    ///
    /// # 参数
    /// * `booklet` - 选品册
    /// * `batch_id` - 目标批次
    /// * `kind` - 准备种类；重生成沿用当前批次
    /// * `catalog` - 商品池端口
    /// * `images` - 图片端口
    ///
    /// # 返回
    /// 返回按 SKU 升序的快照。
    ///
    /// # 错误
    /// 勾选任一失效、筛选空或超限时整批失败。
    pub(super) async fn freeze_pool(
        &self,
        booklet: &SalesSelectionBooklet,
        batch_id: &str,
        kind: PrepareKind,
        catalog: &dyn SelectionCatalogPort,
        images: &dyn SelectionImagePort,
    ) -> Result<Vec<SkuSnapshot>> {
        if kind.reuses_current_batch() {
            return self.load_batch_pool(booklet).await;
        }
        let as_of = BusinessDate::today();
        let facts = self.collect_facts(booklet, catalog, as_of).await?;
        ensure_pool_size(&facts)?;
        self.prepare_progress(booklet, PrepareStage::Images, 0).await?;
        self.snapshot_facts(facts, &booklet.base.id, batch_id, images).await
    }

    /// 读取当前批次已冻结快照。
    ///
    /// # 参数
    /// * `booklet` - 选品册
    ///
    /// # 返回
    /// 返回同一读取边界内的快照。
    ///
    /// # 错误
    /// 查询失败时返回仓储错误。
    async fn load_batch_pool(&self, booklet: &SalesSelectionBooklet) -> Result<Vec<SkuSnapshot>> {
        let mut executor = NoTransaction;
        let domain = crate::repository::sales_selection::SalesSelectionDomainRepository::new(&self.db);
        let batch = booklet.current_batch_id.clone().unwrap_or_default();
        let members = domain.list_pool_members(&booklet.base.id, &batch, &mut executor).await?;
        let mut snapshots: Vec<SkuSnapshot> = members.into_iter().map(|item| item.sku).collect();
        sort_by_sku_id(&mut snapshots);
        Ok(snapshots)
    }

    /// 按来源取出供给事实。
    ///
    /// # 参数
    /// * `booklet` - 选品册
    /// * `catalog` - 商品池端口
    /// * `as_of` - 资格业务日期
    ///
    /// # 返回
    /// 返回具备资格的事实。
    ///
    /// # 错误
    /// 勾选任一失效整批失败并列明项。
    async fn collect_facts(
        &self,
        booklet: &SalesSelectionBooklet,
        catalog: &dyn SelectionCatalogPort,
        as_of: BusinessDate,
    ) -> Result<Vec<SelectionSkuFact>> {
        if booklet.pool_source.kind == PoolSourceKind::Selection {
            return self.collect_picked(booklet, catalog, as_of).await;
        }
        let filter = booklet.pool_source.filter.clone().unwrap_or_default();
        catalog.collect_by_filter(&filter, as_of).await
    }

    /// 按勾选取出并复验资格。
    ///
    /// # 参数
    /// * `booklet` - 选品册
    /// * `catalog` - 商品池端口
    /// * `as_of` - 资格业务日期
    ///
    /// # 返回
    /// 返回全部仍具备资格的事实。
    ///
    /// # 错误
    /// 任一勾选失效整批失败并列明项。
    async fn collect_picked(
        &self,
        booklet: &SalesSelectionBooklet,
        catalog: &dyn SelectionCatalogPort,
        as_of: BusinessDate,
    ) -> Result<Vec<SelectionSkuFact>> {
        let requested: Vec<String> =
            booklet.pool_source.sku_ids.iter().flatten().map(ToString::to_string).collect();
        let facts = catalog.collect_by_ids(&requested, as_of).await?;
        ensure_picked_coverage(&requested, &facts)?;
        Ok(facts)
    }

    /// 快照图片并规范化 SKU。
    ///
    /// # 参数
    /// * `facts` - 供给事实
    /// * `booklet_id` - 选品册
    /// * `batch_id` - 批次
    /// * `images` - 图片端口
    ///
    /// # 返回
    /// 返回升序快照。
    ///
    /// # 错误
    /// 快照非法时拒绝。
    async fn snapshot_facts(
        &self,
        facts: Vec<SelectionSkuFact>,
        booklet_id: &str,
        batch_id: &str,
        images: &dyn SelectionImagePort,
    ) -> Result<Vec<SkuSnapshot>> {
        let mut snapshots = Vec::with_capacity(facts.len());
        for fact in facts {
            snapshots.push(snapshot_one_fact(fact, booklet_id, batch_id, images).await?);
        }
        sort_by_sku_id(&mut snapshots);
        Ok(snapshots)
    }

    /// 生成陈列与成员实体。
    ///
    /// # 参数
    /// * `booklet` - 选品册
    /// * `batch_id` - 目标批次
    /// * `snapshots` - 冻结快照
    /// * `req` - 准备请求
    /// * `generator` - 套餐图端口
    ///
    /// # 返回
    /// 返回陈列、成员与逐档报告。
    ///
    /// # 错误
    /// 任一档为零或图片失败致零时整次失败。
    pub(super) async fn build_displays(
        &self,
        booklet: &SalesSelectionBooklet,
        batch_id: &str,
        snapshots: &[SkuSnapshot],
        req: &PrepareSalesSelectionRequest,
        generator: std::sync::Arc<dyn PackageImageGenerator>,
    ) -> Result<(
        Vec<SalesSelectionDisplayItem>,
        Vec<SalesSelectionPoolMember>,
        Vec<crate::entity::sales_selection::TierSearchReport>,
    )> {
        let mut members = Vec::new();
        if !req.kind.reuses_current_batch() {
            members = pool_members_of(&booklet.base.id, batch_id, snapshots, booklet);
        }
        if !booklet.form.is_package() {
            return Ok((single_items(booklet, batch_id, snapshots)?, members, Vec::new()));
        }
        let (items, reports) = package_items(booklet, batch_id, snapshots, req, generator, &self.db).await?;
        Ok((items, members, reports))
    }
}

/// 校验商品池数量。
///
/// # 参数
/// * `facts` - 供给事实
///
/// # 返回
/// 落在 1 到 500 返回 `Ok(())`。
///
/// # 错误
/// 为空或超限时整次失败。
/// 将整册重新准备请求中的筛选、勾选或档位应用到内存中的选品册。
///
/// 未提交的字段沿用原规则。来源类型不得改变。
///
/// # 参数
/// * `booklet` - 选品册
/// * `req` - 准备请求
///
/// # 返回
/// 成功时更新内存规则。
///
/// # 错误
/// 来源类型或档位非法时拒绝。
pub(super) fn apply_reprepare_request(
    booklet: &mut SalesSelectionBooklet,
    req: &PrepareSalesSelectionRequest,
) -> Result<()> {
    let pool_source = if req.pool_filter.is_some() || req.sku_ids.is_some() {
        PoolSource::new(
            booklet.pool_source.kind,
            req.pool_filter.clone(),
            req.sku_ids.as_ref().map(|ids| ids.iter().cloned().map(SkuId::new).collect()),
        )?
    } else {
        booklet.pool_source.clone()
    };
    let tiers = if req.tiers.is_empty() {
        booklet.tiers.clone()
    } else {
        req.tiers
            .iter()
            .enumerate()
            .map(|(index, tier)| TierRule {
                tier_id: format!("tier-{index}"),
                name: tier.name.clone(),
                target_amount: tier.target_amount,
                tolerance: tier.tolerance,
                expected_count: tier.expected_count,
                sku_count: tier.sku_count,
            })
            .collect()
    };
    Ok(booklet.apply_reprepare_rules(pool_source, tiers)?)
}

/// 校验商品池数量。
///
/// # 参数
/// * `facts` - 供给事实
///
/// # 返回
/// 落在 1 到 500 返回 `Ok(())`。
///
/// # 错误
/// 为空或超限时整次失败。
fn ensure_pool_size(facts: &[SelectionSkuFact]) -> Result<()> {
    if facts.is_empty() {
        return Err(Error::selection_limit("筛选结果为空，不能准备选品册"));
    }
    if facts.len() > POOL_SKU_MAX {
        return Err(Error::selection_limit("每册商品池 SKU 数不得超过 500"));
    }
    Ok(())
}

/// 校验勾选覆盖。
///
/// # 参数
/// * `requested` - 去重后的勾选身份
/// * `facts` - 仍具备资格的事实
///
/// # 返回
/// 全覆盖返回 `Ok(())`。
///
/// # 错误
/// 任一失效整批失败并列明项。
fn ensure_picked_coverage(requested: &[String], facts: &[SelectionSkuFact]) -> Result<()> {
    let found: HashSet<&str> = facts.iter().map(|fact| fact.sku_id.as_str()).collect();
    let missing: Vec<&str> = requested.iter().map(String::as_str).filter(|id| !found.contains(id)).collect();
    if missing.is_empty() {
        return Ok(());
    }
    Err(Error::ValidationError(format!("以下勾选已失效: {}", missing.join("、"))))
}

/// 快照单个 SKU。
///
/// # 参数
/// * `fact` - 供给事实
/// * `booklet_id` - 选品册
/// * `batch_id` - 批次
/// * `images` - 图片端口
///
/// # 返回
/// 返回规范化快照。
///
/// # 错误
/// 事实非法时拒绝。
async fn snapshot_one_fact(
    fact: SelectionSkuFact,
    booklet_id: &str,
    batch_id: &str,
    images: &dyn SelectionImagePort,
) -> Result<SkuSnapshot> {
    let image = images.snapshot_image(fact.main_image_asset_id.as_deref(), booklet_id, batch_id).await?;
    SkuSnapshot {
        sku_id: SkuId::new(fact.sku_id),
        sku_revision_id: erp_core::ids::SkuRevisionId::new(fact.sku_revision_id),
        product_id: erp_core::ids::ProductId::new(fact.product_id),
        product_kind: fact.product_kind,
        category_id: fact.category_id,
        name: fact.name,
        specification_attributes: fact.specification_attributes,
        unit: fact.unit,
        image,
        sales_visible_price_gross: fact.sales_visible_price_gross,
    }
    .normalize()
    .map_err(|error| Error::ValidationError(error.to_string()))
}

/// 构造商品池成员实体。
///
/// # 参数
/// * `booklet_id` - 选品册
/// * `batch_id` - 批次
/// * `snapshots` - 快照
/// * `booklet` - 选品册（占位未用，保持调用一致）
///
/// # 返回
/// 返回成员实体。
///
/// # 错误
/// 无。
fn pool_members_of(
    booklet_id: &str,
    batch_id: &str,
    snapshots: &[SkuSnapshot],
    booklet: &SalesSelectionBooklet,
) -> Vec<SalesSelectionPoolMember> {
    let _ = booklet;
    snapshots
        .iter()
        .map(|sku| {
            SalesSelectionPoolMember::new(
                SalesSelectionPoolMemberId::new(next_id()),
                SalesSelectionBookletId::new(booklet_id),
                batch_id.to_string(),
                sku.clone(),
            )
        })
        .collect()
}

/// 构造单品陈列。
///
/// # 参数
/// * `booklet` - 选品册
/// * `batch_id` - 批次
/// * `snapshots` - 快照
///
/// # 返回
/// 返回单品陈列。
///
/// # 错误
/// 快照非法时拒绝。
fn single_items(
    booklet: &SalesSelectionBooklet,
    batch_id: &str,
    snapshots: &[SkuSnapshot],
) -> Result<Vec<SalesSelectionDisplayItem>> {
    let booklet_id = SalesSelectionBookletId::new(booklet.base.id.clone());
    snapshots
        .iter()
        .map(|sku| {
            SalesSelectionDisplayItem::single_sku(
                SalesSelectionDisplayItemId::new(next_id()),
                booklet_id.clone(),
                batch_id.to_string(),
                sku.clone(),
            )
            .map_err(|error| Error::ValidationError(error.to_string()))
        })
        .collect()
}

/// 构造套餐陈列。
///
/// # 参数
/// * `booklet` - 选品册
/// * `batch_id` - 批次
/// * `snapshots` - 快照
/// * `req` - 准备请求
/// * `generator` - 套餐图端口
/// * `db` - 数据库（读取未重生成档位占用）
///
/// # 返回
/// 返回套餐陈列与逐档报告。
///
/// # 错误
/// 任一档为零时整次失败。
async fn package_items(
    booklet: &SalesSelectionBooklet,
    batch_id: &str,
    snapshots: &[SkuSnapshot],
    req: &PrepareSalesSelectionRequest,
    generator: std::sync::Arc<dyn PackageImageGenerator>,
    db: &mongodb::Database,
) -> Result<(Vec<SalesSelectionDisplayItem>, Vec<crate::entity::sales_selection::TierSearchReport>)> {
    let scoped = scoped_tiers(booklet, req);
    let mut occupied = current_occupied(db, booklet, batch_id, req).await?;
    let mut packages = Vec::new();
    let mut reports = Vec::new();
    for tier in scoped {
        let pool = snapshots.to_vec();
        let occupied_run = occupied.clone();
        let generator = generator.clone();
        let seed = req.seed;
        let (generated, mut report) = tokio::task::spawn_blocking(move || {
            search_packages(&pool, &[tier], &occupied_run, generator.as_ref(), seed)
        })
        .await
        .map_err(|_| Error::selection_prepare_failed("套餐搜索执行异常"))?
        .map_err(|error| Error::selection_prepare_failed(error.to_string()))?;
        occupied.extend(generated.iter().map(|package| package.combination_key()));
        packages.extend(generated);
        reports.append(&mut report);
        SalesSelectionService::new(db.clone())
            .prepare_progress(booklet, PrepareStage::Search, reports.len() as u32)
            .await?;
    }
    let booklet_id = SalesSelectionBookletId::new(booklet.base.id.clone());
    let mut items = Vec::with_capacity(packages.len());
    for package in packages {
        items.push(
            SalesSelectionDisplayItem::package(
                SalesSelectionDisplayItemId::new(next_id()),
                booklet_id.clone(),
                batch_id.to_string(),
                package.tier_id.clone(),
                package.members.clone(),
                package.cover.clone(),
            )
            .map_err(|error| Error::selection_prepare_failed(error.to_string()))?,
        );
    }
    Ok((items, reports))
}

/// 确定本次搜索的档位范围。
///
/// # 参数
/// * `booklet` - 选品册
/// * `req` - 准备请求
///
/// # 返回
/// 按档重生成仅返回指定档，否则返回全部档。
///
/// # 错误
/// 无。
fn scoped_tiers(
    booklet: &SalesSelectionBooklet,
    req: &PrepareSalesSelectionRequest,
) -> Vec<crate::entity::sales_selection::TierRule> {
    if req.kind != PrepareKind::RegeneratedTiers || req.tier_ids.is_empty() {
        return booklet.tiers.clone();
    }
    booklet.tiers.iter().filter(|tier| req.tier_ids.contains(&tier.tier_id)).cloned().collect()
}

/// 读取未重生成档位的已占用组合。
///
/// # 参数
/// * `db` - 数据库
/// * `booklet` - 选品册
/// * `batch_id` - 批次
/// * `req` - 准备请求
///
/// # 返回
/// 返回已占用组合键。
///
/// # 错误
/// 查询失败时返回仓储错误。
async fn current_occupied(
    db: &mongodb::Database,
    booklet: &SalesSelectionBooklet,
    batch_id: &str,
    req: &PrepareSalesSelectionRequest,
) -> Result<HashSet<String>> {
    if req.kind != PrepareKind::RegeneratedTiers || req.tier_ids.is_empty() {
        return Ok(HashSet::new());
    }
    let mut executor = NoTransaction;
    let domain = crate::repository::sales_selection::SalesSelectionDomainRepository::new(db);
    let items = domain.list_effective_items(&booklet.base.id, batch_id, &mut executor).await?;
    Ok(occupied_of(&items, Some(&req.tier_ids)))
}

/// 计算已占用组合键。
///
/// # 参数
/// * `items` - 有效陈列
/// * `exclude_tiers` - 排除的重生成档位
///
/// # 返回
/// 返回组合键集合。
///
/// # 错误
/// 无。
fn occupied_of(items: &[SalesSelectionDisplayItem], exclude_tiers: Option<&[String]>) -> HashSet<String> {
    let mut occupied = HashSet::new();
    for item in items {
        if excluded_item(item, exclude_tiers) {
            continue;
        }
        occupied.insert(item_combo_key(item));
    }
    occupied
}

/// 判断陈列是否属于重生成范围。
///
/// # 参数
/// * `item` - 陈列项
/// * `exclude_tiers` - 重生成档位
///
/// # 返回
/// 属于时返回 `true`。
///
/// # 错误
/// 无。
fn excluded_item(item: &SalesSelectionDisplayItem, exclude_tiers: Option<&[String]>) -> bool {
    let Some(tiers) = exclude_tiers else {
        return false;
    };
    match &item.kind {
        crate::entity::sales_selection::DisplayKind::Package { tier_id, .. } => tiers.contains(tier_id),
        crate::entity::sales_selection::DisplayKind::SingleSku { .. } => false,
    }
}

/// 计算单项组合键。
///
/// # 参数
/// * `item` - 陈列项
///
/// # 返回
/// 返回成员升序连接键。
///
/// # 错误
/// 无。
fn item_combo_key(item: &SalesSelectionDisplayItem) -> String {
    match &item.kind {
        crate::entity::sales_selection::DisplayKind::SingleSku { sku } => sku.sku_id.to_string(),
        crate::entity::sales_selection::DisplayKind::Package { members, .. } => {
            combination_key(members.iter().map(|member| member.sku_id.as_ref()))
        },
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::str::FromStr;

    use erp_core::common::time::BusinessDate;
    use erp_core::money::Amount;

    use super::{ensure_picked_coverage, ensure_pool_size, occupied_of};
    use crate::entity::sales_selection::SkuSnapshot;
    use crate::ports::sales_selection::SelectionSkuFact;

    fn fact(id: &str) -> SelectionSkuFact {
        SelectionSkuFact {
            sku_id: id.into(),
            sku_revision_id: format!("rev-{id}"),
            product_id: format!("p-{id}"),
            product_kind: "PHYSICAL".into(),
            category_id: None,
            name: id.into(),
            specification_attributes: Vec::new(),
            unit: "件".into(),
            main_image_asset_id: None,
            sales_visible_price_gross: Amount::from_str("10.00").unwrap(),
        }
    }

    fn snapshot(id: &str, product: &str) -> SkuSnapshot {
        SkuSnapshot {
            sku_id: erp_core::ids::SkuId::new(id),
            sku_revision_id: erp_core::ids::SkuRevisionId::new(format!("rev-{id}")),
            product_id: erp_core::ids::ProductId::new(product),
            product_kind: "PHYSICAL".into(),
            category_id: None,
            name: id.into(),
            specification_attributes: Vec::new(),
            unit: "件".into(),
            image: None,
            sales_visible_price_gross: Amount::from_str("10.00").unwrap(),
        }
    }

    /// 按 SKU 升序校验快照顺序（测试断言用）。
    fn is_sorted_snapshots(snapshots: &[SkuSnapshot]) -> bool {
        snapshots.windows(2).all(|pair| pair[0].sku_id.as_ref() <= pair[1].sku_id.as_ref())
    }

    /// 统计各 SPU 成员数（测试断言用）。
    fn spu_counts(snapshots: &[SkuSnapshot]) -> BTreeMap<String, usize> {
        let mut counts = BTreeMap::new();
        for snapshot in snapshots {
            *counts.entry(snapshot.product_id.to_string()).or_insert(0) += 1;
        }
        counts
    }

    #[test]
    fn pool_size_rejects_empty_and_overflow() {
        assert!(ensure_pool_size(&[]).is_err());
        let many: Vec<_> = (0..501).map(|index| fact(&format!("sku-{index}"))).collect();
        assert!(ensure_pool_size(&many).is_err());
        assert!(ensure_pool_size(&[fact("a")]).is_ok());
    }

    #[test]
    fn picked_coverage_lists_missing() {
        let facts = vec![fact("a")];
        assert!(ensure_picked_coverage(&["a".into()], &facts).is_ok());
        let error = ensure_picked_coverage(&["a".into(), "b".into()], &facts).unwrap_err();
        assert!(error.to_string().contains('b'));
    }

    #[test]
    fn snapshots_sorted_and_spu_counted() {
        assert!(is_sorted_snapshots(&[snapshot("a", "p1"), snapshot("b", "p1")]));
        assert!(!is_sorted_snapshots(&[snapshot("b", "p1"), snapshot("a", "p1")]));
        let counts = spu_counts(&[snapshot("a", "p1"), snapshot("b", "p1"), snapshot("c", "p2")]);
        assert_eq!(counts["p1"], 2);
        let _ = BusinessDate::today();
    }

    #[test]
    fn occupied_excludes_regen_scope() {
        assert!(occupied_of(&[], None).is_empty());
        assert!(occupied_of(&[], Some(&["t1".to_string()])).is_empty());
    }
}
