//! 套餐组合搜索：有界搜索，不是笛卡尔积。

use std::collections::{BTreeSet, HashSet};
use std::time::{Duration, Instant};

use super::image::{PackageCoverRef, PackageImageGenerator};
use super::limits::{
    COMBINATION_ALGORITHM_VERSION, PACKAGE_DISPLAY_MAX, SEARCH_STATE_BUDGET, SEARCH_TIME_BUDGET_SECS,
};
use super::pricing::{abs_diff, price_in_tier};
use super::sku_snapshot::{package_price, SkuSnapshot};
use super::tier::TierRule;
use super::types::SearchStopReason;
use erp_core::money::Amount;
use erp_core::{Error, Result};

/// 一档搜索统计。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TierSearchReport {
    /// 档位身份。
    pub tier_id: String,
    /// 期望数量。
    pub expected_count: u32,
    /// 实际可交付数量。
    pub actual_count: u32,
    /// 停止原因。
    pub stop_reason: SearchStopReason,
    /// 展开状态数。
    pub expanded_states: u64,
    /// 耗时毫秒。
    pub elapsed_ms: u64,
    /// 图片失败数量。
    pub image_failures: u32,
    /// 算法版本。
    pub algorithm_version: String,
    /// 搜索种子。
    pub seed: u64,
}

/// 一次生成的套餐。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeneratedPackage {
    /// 所属档位。
    pub tier_id: String,
    /// 按稳定 SKU 身份升序排列的成员。
    pub members: Vec<SkuSnapshot>,
    /// 套餐售价。
    pub price: Amount,
    /// 套餐主图。
    pub cover: PackageCoverRef,
}

impl GeneratedPackage {
    /// 返回组合去重键。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回成员稳定 SKU 身份以 `|` 连接的升序键。
    ///
    /// # 错误
    /// 无。
    pub fn combination_key(&self) -> String {
        combination_key(self.members.iter().map(|item| item.sku_id_str()))
    }
}

/// 计算组合去重键。
///
/// # 参数
/// * `sku_ids` - 成员稳定身份
///
/// # 返回
/// 排序后连接的键。同一集合无论顺序均为同一键。
///
/// # 错误
/// 无。
pub fn combination_key<'a, I>(sku_ids: I) -> String
where
    I: IntoIterator<Item = &'a str>,
{
    let mut ids: Vec<&str> = sku_ids.into_iter().collect();
    ids.sort_unstable();
    ids.join("|")
}

/// 按档位顺序搜索套餐。
///
/// # 参数
/// * `pool` - 已按稳定 SKU 身份升序的商品池
/// * `tiers` - 创建顺序的档位
/// * `occupied` - 其他档已占用的组合
/// * `generator` - 套餐图生成端口
/// * `seed` - 搜索种子；P0 算法确定，种子只记录
///
/// # 返回
/// 返回可交付套餐与逐档报告。任一档 0 套时返回错误，调用方不得提交部分新结果。
///
/// # 错误
/// 池为空、SKU 数超过池大小、某档 0 套或金额溢出时失败。
pub fn search_packages(
    pool: &[SkuSnapshot],
    tiers: &[TierRule],
    occupied: &HashSet<String>,
    generator: &dyn PackageImageGenerator,
    seed: u64,
) -> Result<(Vec<GeneratedPackage>, Vec<TierSearchReport>)> {
    if pool.is_empty() {
        return Err(Error::from("商品池不能为空"));
    }
    let mut occupied = occupied.clone();
    let mut packages = Vec::new();
    let mut reports = Vec::with_capacity(tiers.len());
    for tier in tiers {
        if usize::try_from(tier.sku_count).unwrap_or(usize::MAX) > pool.len() {
            return Err(Error::from("每套餐独立 SKU 数不得超过商品池 SKU 数"));
        }
        let (tier_packages, report) = search_one_tier(pool, tier, &occupied, generator, seed)?;
        if tier_packages.is_empty() {
            return Err(Error::from(format!(
                "档位「{}」未得到可交付套餐，整次准备失败",
                tier.name
            )));
        }
        for package in &tier_packages {
            occupied.insert(package.combination_key());
        }
        packages.extend(tier_packages);
        reports.push(report);
    }
    if packages.len() > PACKAGE_DISPLAY_MAX {
        return Err(Error::from("套餐陈列项数不得超过 200"));
    }
    Ok((packages, reports))
}

/// 搜索一档。
///
/// # 参数
/// * `pool` - 商品池
/// * `tier` - 档位规则
/// * `occupied` - 已占用组合
/// * `generator` - 图片端口
/// * `seed` - 搜索种子
///
/// # 返回
/// 返回本档可交付套餐与报告。允许不足期望数量。
///
/// # 错误
/// 金额溢出时失败。
fn search_one_tier(
    pool: &[SkuSnapshot],
    tier: &TierRule,
    occupied: &HashSet<String>,
    generator: &dyn PackageImageGenerator,
    seed: u64,
) -> Result<(Vec<GeneratedPackage>, TierSearchReport)> {
    let k = usize::try_from(tier.sku_count).unwrap_or(0);
    let started = Instant::now();
    let scope = DfsScope {
        pool,
        tier,
        occupied,
        started,
    };
    let mut progress = DfsProgress {
        expanded: 0,
        stopped: SearchStopReason::ExhaustedSpace,
        candidates: Vec::new(),
    };
    dfs_collect(&scope, &mut progress, k, 0, Amount::zero(), &mut Vec::new())?;
    let DfsProgress {
        expanded,
        mut stopped,
        mut candidates,
    } = progress;
    candidates.sort_by(|left, right| compare_preference(left, right, &[], tier.target_amount, pool));
    let mut accepted = Vec::new();
    let mut image_failures = 0;
    while !candidates.is_empty() {
        let keys: Vec<String> = accepted.iter().map(GeneratedPackage::combination_key).collect();
        let best = candidates
            .iter()
            .enumerate()
            .min_by(|(_, left), (_, right)| compare_preference(left, right, &keys, tier.target_amount, pool))
            .map(|(index, _)| index)
            .unwrap_or(0);
        let combo = candidates.swap_remove(best);
        if accepted.len() >= usize::try_from(tier.expected_count).unwrap_or(0) {
            stopped = SearchStopReason::ReachedExpected;
            break;
        }
        match materialize_package(pool, &combo, tier, generator) {
            Ok(package) => {
                let key = package.combination_key();
                if accepted
                    .iter()
                    .any(|item: &GeneratedPackage| item.combination_key() == key)
                {
                    continue;
                }
                accepted.push(package);
            }
            Err(_) => image_failures += 1,
        }
        if accepted.len() >= usize::try_from(tier.expected_count).unwrap_or(0) {
            stopped = SearchStopReason::ReachedExpected;
        }
    }
    let ranked = rerank_accepted(accepted, pool, tier.target_amount, tier.expected_count);
    Ok((
        ranked.clone(),
        TierSearchReport {
            tier_id: tier.tier_id.clone(),
            expected_count: tier.expected_count,
            actual_count: u32::try_from(ranked.len()).unwrap_or(0),
            stop_reason: stopped,
            expanded_states: expanded,
            elapsed_ms: u64::try_from(scope.started.elapsed().as_millis()).unwrap_or(u64::MAX),
            image_failures,
            algorithm_version: COMBINATION_ALGORITHM_VERSION.to_string(),
            seed,
        },
    ))
}

/// 深度优先搜索共享只读上下文。
///
/// # 参数
/// 无（字段级文档见各字段）。
///
/// # 返回
/// 无。
///
/// # 错误
/// 无。
struct DfsScope<'a> {
    /// 商品池。
    pool: &'a [SkuSnapshot],
    /// 档位规则。
    tier: &'a TierRule,
    /// 已占用组合。
    occupied: &'a HashSet<String>,
    /// 开始时刻。
    started: Instant,
}

/// 深度优先搜索可变进度。
///
/// # 参数
/// 无（字段级文档见各字段）。
///
/// # 返回
/// 无。
///
/// # 错误
/// 无。
struct DfsProgress {
    /// 展开状态计数。
    expanded: u64,
    /// 停止原因。
    stopped: SearchStopReason,
    /// 合法完整组合。
    candidates: Vec<Vec<usize>>,
}

/// 深度优先搜索合法组合。
///
/// # 参数
/// * `scope` - 共享只读上下文
/// * `progress` - 可变进度
/// * `remaining` - 仍需挑选的成员数
/// * `start` - 下一个可取下标
/// * `sum` - 已选售价之和
/// * `chosen` - 已选下标
///
/// # 返回
/// 预算内收集候选。
///
/// # 错误
/// 金额溢出时失败。
fn dfs_collect(
    scope: &DfsScope<'_>,
    progress: &mut DfsProgress,
    remaining: usize,
    start: usize,
    sum: Amount,
    chosen: &mut Vec<usize>,
) -> Result<()> {
    progress.expanded += 1;
    if progress.expanded >= SEARCH_STATE_BUDGET {
        progress.stopped = SearchStopReason::BudgetStates;
        return Ok(());
    }
    if scope.started.elapsed() >= Duration::from_secs(SEARCH_TIME_BUDGET_SECS) {
        progress.stopped = SearchStopReason::BudgetTime;
        return Ok(());
    }
    if remaining == 0 {
        record_complete_candidate(
            scope.pool,
            chosen,
            scope.tier,
            scope.occupied,
            sum,
            &mut progress.candidates,
        )?;
        return Ok(());
    }
    let available = scope.pool.len().saturating_sub(start);
    if available < remaining {
        return Ok(());
    }
    if !can_still_hit_range(scope.pool, start, remaining, sum, scope.tier)? {
        return Ok(());
    }
    for index in start..scope.pool.len() {
        if matches!(
            progress.stopped,
            SearchStopReason::BudgetStates | SearchStopReason::BudgetTime
        ) {
            return Ok(());
        }
        let next_sum = super::pricing::try_add(sum, scope.pool[index].sales_visible_price_gross)?;
        chosen.push(index);
        dfs_collect(scope, progress, remaining - 1, index + 1, next_sum, chosen)?;
        chosen.pop();
    }
    Ok(())
}

/// 记录一个完整合法候选。
///
/// # 参数
/// * `pool` - 商品池
/// * `chosen` - 成员下标
/// * `tier` - 档位
/// * `occupied` - 已占用组合
/// * `sum` - 售价
/// * `candidates` - 输出
///
/// # 返回
/// 合法且未占用时写入候选。
///
/// # 错误
/// 区间上界溢出时失败。
fn record_complete_candidate(
    pool: &[SkuSnapshot],
    chosen: &[usize],
    tier: &TierRule,
    occupied: &HashSet<String>,
    sum: Amount,
    candidates: &mut Vec<Vec<usize>>,
) -> Result<()> {
    if !price_in_tier(sum, tier.target_amount, tier.tolerance)? {
        return Ok(());
    }
    let key = combination_key(chosen.iter().map(|index| pool[*index].sku_id_str()));
    if occupied.contains(&key) {
        return Ok(());
    }
    candidates.push(chosen.to_vec());
    Ok(())
}

/// 用后缀最小/最大金额判断是否还能落入容差。
///
/// # 参数
/// * `pool` - 商品池，调用方按 sku_id 排序；本函数按价格取后缀极值
/// * `start` - 剩余起点
/// * `remaining` - 仍需件数
/// * `sum` - 已选金额
/// * `tier` - 档位
///
/// # 返回
/// 仍可能落入区间时返回 `true`。
///
/// # 错误
/// 金额溢出时失败。
fn can_still_hit_range(
    pool: &[SkuSnapshot],
    start: usize,
    remaining: usize,
    sum: Amount,
    tier: &TierRule,
) -> Result<bool> {
    let mut rest: Vec<Amount> = pool[start..]
        .iter()
        .map(|item| item.sales_visible_price_gross)
        .collect();
    if rest.len() < remaining {
        return Ok(false);
    }
    rest.sort();
    let min_add = super::pricing::try_sum(rest.iter().copied().take(remaining))?;
    let max_add = super::pricing::try_sum(rest.iter().rev().copied().take(remaining))?;
    let min_total = super::pricing::try_add(sum, min_add)?;
    let max_total = super::pricing::try_add(sum, max_add)?;
    let lower = if tier.tolerance >= tier.target_amount {
        Amount::zero()
    } else {
        tier.target_amount.checked_sub(tier.tolerance)
    };
    let upper = super::pricing::try_add(tier.target_amount, tier.tolerance)?;
    Ok(max_total >= lower && min_total <= upper)
}

/// 把候选下标物化为带封面的套餐。
///
/// # 参数
/// * `pool` - 商品池
/// * `indexes` - 成员下标
/// * `tier` - 档位
/// * `generator` - 图片端口
///
/// # 返回
/// 返回成员按 sku_id 升序、已写入主图的套餐。
///
/// # 错误
/// 图片端口失败或金额溢出时拒绝该套餐。
fn materialize_package(
    pool: &[SkuSnapshot],
    indexes: &[usize],
    tier: &TierRule,
    generator: &dyn PackageImageGenerator,
) -> Result<GeneratedPackage> {
    let mut members: Vec<SkuSnapshot> = indexes.iter().map(|index| pool[*index].clone()).collect();
    super::sku_snapshot::sort_by_sku_id(&mut members);
    let ids: BTreeSet<&str> = members.iter().map(SkuSnapshot::sku_id_str).collect();
    if ids.len() != members.len() {
        return Err(Error::from("同一套餐内 SKU 不得重复"));
    }
    let price = package_price(&members)?;
    let urls: Vec<Option<String>> = members.iter().map(SkuSnapshot::image_port_url).collect();
    let cover_url = generator.generate(&urls)?;
    let cover = cover_from_port_url(&members, &cover_url, generator.implementation_version())?;
    Ok(GeneratedPackage {
        tier_id: tier.tier_id.clone(),
        members,
        price,
        cover,
    })
}

/// 把端口输出 URL 映射回成员快照资产。
///
/// # 参数
/// * `members` - 已排序成员
/// * `cover_url` - 端口输出
/// * `version` - 实现版本
///
/// # 返回
/// 返回套餐主图引用。
///
/// # 错误
/// 输出无法对应到成员资产时拒绝，防止绕过端口直接写 SKU 主图字段。
fn cover_from_port_url(members: &[SkuSnapshot], cover_url: &str, version: &str) -> Result<PackageCoverRef> {
    let matched = members.iter().find_map(|member| {
        let url = member.image_port_url()?;
        (url == cover_url).then_some(member.image.as_ref()).flatten()
    });
    let image = matched.ok_or_else(|| Error::from("套餐主图必须经生成端口写入"))?;
    PackageCoverRef::from_port_output(
        image.file_asset_id.clone(),
        image.content_checksum.clone(),
        image.storage_object_key.clone(),
        version.to_string(),
    )
}

/// 偏好排序。
///
/// # 参数
/// * `left` / `right` - 成员下标
/// * `accepted` - 本档已接纳套餐的组合键
/// * `target` - 目标金额
/// * `pool` - 商品池
///
/// # 返回
/// 按重复 SPU、分类种类、重合、差额、SKU 序列排序。
///
/// # 错误
/// 无。
fn compare_preference(
    left: &[usize],
    right: &[usize],
    accepted: &[String],
    target: Amount,
    pool: &[SkuSnapshot],
) -> std::cmp::Ordering {
    duplicate_spu_count(left, pool)
        .cmp(&duplicate_spu_count(right, pool))
        .then_with(|| known_category_count(right, pool).cmp(&known_category_count(left, pool)))
        .then_with(|| max_overlap(left, accepted, pool).cmp(&max_overlap(right, accepted, pool)))
        .then_with(|| {
            abs_diff(index_price(left, pool), target).cmp(&abs_diff(index_price(right, pool), target))
        })
        .then_with(|| sku_id_seq(left, pool).cmp(&sku_id_seq(right, pool)))
}

/// 计算重复 SPU 成员数。
///
/// # 参数
/// * `indexes` - 成员下标
/// * `pool` - 商品池
///
/// # 返回
/// 返回 `sum(count - 1)`，同 SPU 只出现一次则为 0。
///
/// # 错误
/// 无。
fn duplicate_spu_count(indexes: &[usize], pool: &[SkuSnapshot]) -> u32 {
    let mut counts = std::collections::BTreeMap::<&str, u32>::new();
    for index in indexes {
        *counts.entry(pool[*index].product_id.as_ref()).or_insert(0) += 1;
    }
    counts.values().map(|count| count.saturating_sub(1)).sum()
}

/// 计算已知分类种类。
///
/// # 参数
/// * `indexes` - 成员下标
/// * `pool` - 商品池
///
/// # 返回
/// 缺失分类不计为一个新分类。
///
/// # 错误
/// 无。
fn known_category_count(indexes: &[usize], pool: &[SkuSnapshot]) -> u32 {
    let set: BTreeSet<&str> = indexes
        .iter()
        .filter_map(|index| pool[*index].category_id.as_deref())
        .collect();
    u32::try_from(set.len()).unwrap_or(0)
}

/// 与已接纳套餐的最大 SKU 重合数。
///
/// # 参数
/// * `indexes` - 成员下标
/// * `accepted` - 已接纳组合键
/// * `pool` - 商品池
///
/// # 返回
/// 尚无套餐时返回 0。
///
/// # 错误
/// 无。
fn max_overlap(indexes: &[usize], accepted: &[String], pool: &[SkuSnapshot]) -> u32 {
    if accepted.is_empty() {
        return 0;
    }
    let current: BTreeSet<&str> = indexes.iter().map(|index| pool[*index].sku_id_str()).collect();
    accepted
        .iter()
        .map(|key| key.split('|').filter(|sku_id| current.contains(*sku_id)).count())
        .max()
        .unwrap_or(0) as u32
}

/// 候选售价。
///
/// # 参数
/// * `indexes` - 成员下标
/// * `pool` - 商品池
///
/// # 返回
/// 返回成员售价之和；测试数据不会溢出。
///
/// # 错误
/// 无。
fn index_price(indexes: &[usize], pool: &[SkuSnapshot]) -> Amount {
    super::pricing::try_sum(indexes.iter().map(|index| pool[*index].sales_visible_price_gross))
        .unwrap_or(Amount::zero())
}

/// 成员 SKU 身份序列。
///
/// # 参数
/// * `indexes` - 成员下标
/// * `pool` - 商品池
///
/// # 返回
/// 返回升序身份向量。
///
/// # 错误
/// 无。
fn sku_id_seq(indexes: &[usize], pool: &[SkuSnapshot]) -> Vec<String> {
    let mut ids: Vec<String> = indexes
        .iter()
        .map(|index| pool[*index].sku_id_str().to_string())
        .collect();
    ids.sort();
    ids
}

/// 从已接纳套餐取成员下标。
///
/// # 参数
/// * `pool` - 商品池
/// * `package` - 套餐
///
/// # 返回
/// 返回成员在池中的下标。
///
/// # 错误
/// 无。
fn member_indexes(pool: &[SkuSnapshot], package: &GeneratedPackage) -> Vec<usize> {
    package
        .members
        .iter()
        .filter_map(|member| pool.iter().position(|item| item.sku_id == member.sku_id))
        .collect()
}

/// 按已接纳集合重新择优直到达到期望数量。
///
/// # 参数
/// * `accepted` - 已通过图片的套餐
/// * `pool` - 商品池
/// * `target` - 目标金额
/// * `expected` - 期望数量
///
/// # 返回
/// 返回择优后的有序列表。
///
/// # 错误
/// 无。
fn rerank_accepted(
    accepted: Vec<GeneratedPackage>,
    pool: &[SkuSnapshot],
    target: Amount,
    expected: u32,
) -> Vec<GeneratedPackage> {
    let mut remaining = accepted;
    let mut selected = Vec::new();
    let limit = usize::try_from(expected).unwrap_or(remaining.len());
    while selected.len() < limit && !remaining.is_empty() {
        let keys: Vec<String> = selected.iter().map(GeneratedPackage::combination_key).collect();
        remaining.sort_by(|left, right| {
            compare_preference(
                &member_indexes(pool, left),
                &member_indexes(pool, right),
                &keys,
                target,
                pool,
            )
        });
        selected.push(remaining.remove(0));
    }
    selected
}

#[cfg(test)]
mod tests {
    use super::{combination_key, search_packages};
    use crate::entity::sales_selection::image::FirstNonEmptyMemberImage;
    use crate::entity::sales_selection::sku_snapshot::{ImageAssetSnapshot, SkuSnapshot};
    use crate::entity::sales_selection::tier::TierRule;
    use erp_core::ids::{ProductId, SkuId, SkuRevisionId};
    use erp_core::money::Amount;
    use std::collections::HashSet;
    use std::str::FromStr;

    fn amount(value: &str) -> Amount {
        Amount::from_str(value).unwrap()
    }

    fn sku(id: &str, product: &str, price: &str, image: bool) -> SkuSnapshot {
        SkuSnapshot {
            sku_id: SkuId::new(id),
            sku_revision_id: SkuRevisionId::new(format!("rev-{id}")),
            product_id: ProductId::new(product),
            product_kind: "PHYSICAL".into(),
            category_id: Some("tea".into()),
            name: id.into(),
            specification_attributes: Vec::new(),
            unit: "件".into(),
            image: image.then(|| ImageAssetSnapshot {
                file_asset_id: format!("file-{id}"),
                content_checksum: "abc".into(),
                storage_object_key: format!("key-{id}"),
            }),
            sales_visible_price_gross: amount(price),
        }
    }

    fn tier() -> TierRule {
        TierRule {
            tier_id: "t1".into(),
            name: "100 元档".into(),
            target_amount: amount("100.00"),
            tolerance: amount("5.00"),
            expected_count: 2,
            sku_count: 2,
        }
    }

    #[test]
    fn order_does_not_create_duplicate_combination() {
        assert_eq!(combination_key(["b", "a"]), combination_key(["a", "b"]));
    }

    #[test]
    fn searches_limited_packages_inside_tolerance() {
        let pool = vec![
            sku("a", "p1", "40.00", true),
            sku("b", "p2", "60.00", true),
            sku("c", "p3", "55.00", true),
            sku("d", "p4", "45.00", true),
        ];
        let (packages, reports) =
            search_packages(&pool, &[tier()], &HashSet::new(), &FirstNonEmptyMemberImage, 0).unwrap();
        assert!(!packages.is_empty());
        assert!(packages.len() <= 2);
        for package in &packages {
            assert!(package.price >= amount("95.00") && package.price <= amount("105.00"));
            assert_eq!(package.cover.generator_version, "p0-first-member-v1");
        }
        assert_eq!(reports[0].actual_count, u32::try_from(packages.len()).unwrap());
    }

    #[test]
    fn chooses_disjoint_package_before_an_overlapping_candidate() {
        let pool = vec![
            sku("a", "p1", "50.00", true),
            sku("b", "p2", "50.00", true),
            sku("c", "p3", "50.00", true),
            sku("d", "p4", "50.00", true),
        ];
        let (packages, _) =
            search_packages(&pool, &[tier()], &HashSet::new(), &FirstNonEmptyMemberImage, 0).unwrap();
        assert_eq!(
            packages.iter().map(|p| p.combination_key()).collect::<Vec<_>>(),
            vec!["a|b", "c|d"]
        );
    }

    #[test]
    fn all_members_without_image_fail_the_tier() {
        let pool = vec![sku("a", "p1", "40.00", false), sku("b", "p2", "60.00", false)];
        let error =
            search_packages(&pool, &[tier()], &HashSet::new(), &FirstNonEmptyMemberImage, 0).unwrap_err();
        assert!(error.to_string().contains("未得到可交付套餐"));
    }
}
