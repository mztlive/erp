//! SKU 配送覆盖投影：公司商品池按地区筛选的预算结果。
//!
//! 由后台按供给、可供状态与配送模板的变化重算，允许短暂滞后；采购建单与销售
//! 地址提示仍按实时事实判定，不读取本投影。

use erp_core::change_scan::ChangeWatermarks;
use erp_core::common::time::BusinessDate;
use erp_core::delivery_coverage::DeliveryCoverage;
use erp_core::delivery_policy::DeliveryPolicy;
use serde::{Deserialize, Serialize};

/// 覆盖口径版本；口径变化时递增，后台据此全量重建。
pub const SKU_DELIVERY_COVERAGE_VERSION: i32 = 1;

/// 一个 SKU 的配送覆盖。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkuDeliveryCoverage {
    /// 公司 SKU，即文档主键。
    #[serde(rename = "_id")]
    pub sku_id: String,
    /// 两档可送范围。
    #[serde(flatten)]
    pub coverage: DeliveryCoverage,
    /// 参与计算的有效供给所属供应商；模板变化时据此标记重算。
    pub supplier_ids: Vec<String>,
    /// 名下供给最近一次按日期生效或失效的日期；到期即重算。
    pub next_change_date: Option<BusinessDate>,
    /// 最近一次计算时间（Unix 秒）；未计算过为 0。
    pub computed_at: i64,
    /// 是否待重算。
    pub dirty: bool,
    /// 每次标记待重算递增；保存时比对，避免覆盖计算期间的新标记。
    pub dirty_token: i64,
}

/// 覆盖后台刷新进度。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkuDeliveryCoverageState {
    /// 固定主键。
    #[serde(rename = "_id")]
    pub id: String,
    /// 已完成全量重建的覆盖口径版本。
    pub version: i32,
    /// 变更扫描进度。
    #[serde(flatten)]
    pub watermarks: ChangeWatermarks,
}

impl SkuDeliveryCoverageState {
    /// 刷新进度的固定主键。
    pub const ID: &'static str = "refresh";

    /// 构造刷新进度。
    ///
    /// # 参数
    /// * `version` - 覆盖口径版本
    /// * `watermarks` - 变更扫描进度
    ///
    /// # 返回
    /// 返回带固定主键的进度。
    ///
    /// # 错误
    /// 无。
    pub fn new(version: i32, watermarks: ChangeWatermarks) -> Self {
        Self { id: Self::ID.to_string(), version, watermarks }
    }

    /// 判断是否需要全量重建：首次运行或口径版本低于当前版本。
    ///
    /// # 参数
    /// * `state` - 已保存的进度
    ///
    /// # 返回
    /// 需要重建时为真；较新版本写入的进度不触发重建。
    ///
    /// # 错误
    /// 无。
    pub fn needs_rebuild(state: Option<&Self>) -> bool {
        state.is_none_or(|state| state.version < SKU_DELIVERY_COVERAGE_VERSION)
    }

    /// 全量重建开始时的进度。
    ///
    /// # 参数
    /// * `now` - 当前时间（Unix 秒）
    ///
    /// # 返回
    /// 返回当前口径版本、从现在开始扫描的进度。
    ///
    /// # 错误
    /// 无。
    pub fn rebuilt(now: i64) -> Self {
        Self::new(SKU_DELIVERY_COVERAGE_VERSION, ChangeWatermarks::starting_at(now))
    }

    /// 扫描完成后的进度。
    ///
    /// # 参数
    /// * `now` - 本轮扫描使用的当前时间（Unix 秒）
    ///
    /// # 返回
    /// 返回推进后的进度，口径版本不变。
    ///
    /// # 错误
    /// 无。
    pub fn advanced(&self, now: i64) -> Self {
        Self::new(self.version, self.watermarks.advanced(now))
    }
}

impl SkuDeliveryCoverage {
    /// 尚未计算、待重算的占位记录。
    ///
    /// # 参数
    /// * `sku_id` - 公司 SKU
    ///
    /// # 返回
    /// 返回任何地区都不可送、待重算的记录。
    ///
    /// # 错误
    /// 无。
    pub fn pending(sku_id: String) -> Self {
        Self {
            sku_id,
            coverage: DeliveryCoverage::none(),
            supplier_ids: Vec::new(),
            next_change_date: None,
            computed_at: 0,
            dirty: true,
            dirty_token: 1,
        }
    }
}

/// 计算一个 SKU 覆盖所需的事实。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkuDeliveryInput {
    /// 公司 SKU。
    pub sku_id: String,
    /// 是否受配送范围限制：实物商品为真，虚拟商品与线下服务为假。
    pub restricted: bool,
    /// 全部有效且可供供给的生效配送政策，已去重。
    pub policies: Vec<DeliveryPolicy>,
    /// 有效且可供供给的供应商，去重。
    pub supplier_ids: Vec<String>,
    /// 名下供给最近一次按日期生效或失效的日期。
    pub next_change_date: Option<BusinessDate>,
}

impl SkuDeliveryInput {
    /// 按事实计算覆盖。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 没有有效供给时任何地区都不可送；虚拟商品与线下服务不受配送范围限制；
    /// 实物商品按配送政策逐地区判定。
    ///
    /// # 错误
    /// 无。
    pub fn coverage(&self) -> DeliveryCoverage {
        if self.policies.is_empty() {
            DeliveryCoverage::none()
        } else if !self.restricted {
            DeliveryCoverage::unrestricted()
        } else {
            DeliveryCoverage::from_policies(&self.policies.iter().collect::<Vec<_>>())
        }
    }

    /// 由事实生成已计算的覆盖记录。
    ///
    /// # 参数
    /// * `coverage` - 按本事实计算的覆盖
    /// * `computed_at` - 计算时间（Unix 秒）
    ///
    /// # 返回
    /// 返回不再待重算的记录；`dirty_token` 由保存条件保持不变。
    ///
    /// # 错误
    /// 无。
    pub fn into_record(self, coverage: DeliveryCoverage, computed_at: i64) -> SkuDeliveryCoverage {
        SkuDeliveryCoverage {
            sku_id: self.sku_id,
            coverage,
            supplier_ids: self.supplier_ids,
            next_change_date: self.next_change_date,
            computed_at,
            dirty: false,
            dirty_token: 0,
        }
    }
}

/// 同一批计算内按政策组合复用结果：大量 SKU 共用相同模板时只算一次。
#[derive(Debug, Default)]
pub struct SkuDeliveryCoverageMemo {
    entries: Vec<(bool, Vec<DeliveryPolicy>, DeliveryCoverage)>,
}

impl SkuDeliveryCoverageMemo {
    /// 计算或复用一个 SKU 的覆盖。
    ///
    /// # 参数
    /// * `input` - SKU 事实
    ///
    /// # 返回
    /// 返回与 [`SkuDeliveryInput::coverage`] 相同的结果。
    ///
    /// # 错误
    /// 无。
    pub fn coverage(&mut self, input: &SkuDeliveryInput) -> DeliveryCoverage {
        if let Some((_, _, coverage)) = self
            .entries
            .iter()
            .find(|(restricted, policies, _)| *restricted == input.restricted && *policies == input.policies)
        {
            return coverage.clone();
        }
        let coverage = input.coverage();
        self.entries.push((input.restricted, input.policies.clone(), coverage.clone()));
        coverage
    }
}

#[cfg(test)]
mod tests {
    use erp_core::change_scan::ChangeWatermarks;
    use erp_core::delivery_coverage::{DeliveryCoverage, NATIONWIDE_REGION_CODE};
    use erp_core::delivery_policy::{DeliveryPolicy, DeliveryRegionRule, DeliveryRuleSet, DeliveryTerm};
    use erp_core::region::RegionCode;

    use super::{
        SKU_DELIVERY_COVERAGE_VERSION, SkuDeliveryCoverageMemo, SkuDeliveryCoverageState, SkuDeliveryInput,
    };

    fn input(restricted: bool, policies: Vec<DeliveryPolicy>) -> SkuDeliveryInput {
        SkuDeliveryInput {
            sku_id: "sku-1".into(),
            restricted,
            policies,
            supplier_ids: vec!["supplier-1".into()],
            next_change_date: None,
        }
    }

    fn xinjiang_closed() -> DeliveryPolicy {
        DeliveryPolicy::new(
            DeliveryRuleSet {
                default_term: DeliveryTerm::FreightIncluded,
                rules: vec![DeliveryRegionRule {
                    region_code: RegionCode::parse("650000").unwrap(),
                    term: DeliveryTerm::NotDeliverable,
                }],
            },
            DeliveryRuleSet::uniform(DeliveryTerm::NotDeliverable),
        )
        .unwrap()
    }

    #[test]
    fn physical_goods_follow_policies() {
        let coverage = input(true, vec![xinjiang_closed()]).coverage();
        assert!(coverage.included.partial.is_empty());
        assert!(!coverage.included.full.contains(&"650000".to_string()));
        assert!(coverage.included.full.contains(&"440000".to_string()));
    }

    #[test]
    fn virtual_goods_and_services_are_unrestricted_when_supplied() {
        assert_eq!(input(false, vec![xinjiang_closed()]).coverage(), DeliveryCoverage::unrestricted());
        assert_eq!(input(false, Vec::new()).coverage(), DeliveryCoverage::none());
        assert_eq!(input(true, Vec::new()).coverage(), DeliveryCoverage::none());
    }

    #[test]
    fn memo_reuses_identical_policy_sets_only() {
        let mut memo = SkuDeliveryCoverageMemo::default();
        let physical = input(true, vec![xinjiang_closed()]);
        let first = memo.coverage(&physical);
        assert_eq!(memo.coverage(&physical), first);
        let service = input(false, vec![xinjiang_closed()]);
        assert_eq!(memo.coverage(&service).included.full, vec![NATIONWIDE_REGION_CODE.to_string()]);
        assert_eq!(memo.entries.len(), 2);
    }

    #[test]
    fn rebuild_runs_first_time_and_after_version_upgrade_only() {
        assert!(SkuDeliveryCoverageState::needs_rebuild(None));
        let marks = ChangeWatermarks::starting_at(100);
        let older = SkuDeliveryCoverageState::new(SKU_DELIVERY_COVERAGE_VERSION - 1, marks);
        assert!(SkuDeliveryCoverageState::needs_rebuild(Some(&older)));
        let current = SkuDeliveryCoverageState::rebuilt(1_000);
        assert!(!SkuDeliveryCoverageState::needs_rebuild(Some(&current)));
        let newer = SkuDeliveryCoverageState::new(SKU_DELIVERY_COVERAGE_VERSION + 1, marks);
        assert!(!SkuDeliveryCoverageState::needs_rebuild(Some(&newer)));
        assert_eq!(current.watermarks, ChangeWatermarks::starting_at(1_000));
        assert_eq!(current.advanced(1_002).watermarks, ChangeWatermarks::starting_at(1_000).advanced(1_002));
    }

    #[test]
    fn state_keeps_flat_stored_fields() {
        let state = SkuDeliveryCoverageState::rebuilt(1_000);
        let stored = serde_json::to_value(&state).unwrap();
        assert_eq!(stored["fresh_watermark"], 1_000);
        assert_eq!(stored["settled_watermark"], 1_000 - erp_core::change_scan::COMMIT_MARGIN_SECS);
        assert_eq!(serde_json::from_value::<SkuDeliveryCoverageState>(stored).unwrap(), state);
    }

    #[test]
    fn record_is_clean_after_compute() {
        let source = input(true, vec![DeliveryPolicy::nationwide_included()]);
        let coverage = source.coverage();
        let record = source.into_record(coverage.clone(), 1_700_000_000);
        assert!(!record.dirty);
        assert_eq!(record.coverage, coverage);
        assert_eq!(record.supplier_ids, vec!["supplier-1".to_string()]);
    }
}
