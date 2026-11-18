//! 可售城市是独立的来源观察，不改写人工配送模板。
use erp_core::common::time::Instant;
use erp_core::region::RegionCode;
use serde::{Deserialize, Serialize};

use super::SupplierCatalogItem;
use crate::ports::connector::common::Snapshot;
use crate::ports::connector::offer::RegionCoverage;
use crate::{Error, Result};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SourceServiceRegions {
    pub included: Vec<String>,
    pub observed_at: Instant,
}

impl SourceServiceRegions {
    /// 仅接受查询得到的完整允许集合，严格验证标准地区。
    /// # 参数
    /// `snapshot` 为适配器查询，`received_at` 为通知接收时间。
    /// # 返回
    /// 去重排序的完整来源地区，保留空集合。
    /// # 错误
    /// 未报告、非允许集合、过旧观察或地区无法识别时拒绝。
    pub fn queried(snapshot: Snapshot<RegionCoverage>, received_at: Instant) -> Result<Self> {
        let RegionCoverage::Included(mut included) = snapshot.value else {
            return Err(Error::ValidationError("商品城市补查未返回完整允许地区集合".into()));
        };
        if snapshot.stamp.observed_at < received_at {
            return Err(Error::ConflictError("商品城市观察早于回调".into()));
        }

        for code in &included {
            RegionCode::parse(code)?;
        }
        included.sort();
        included.dedup();
        Ok(Self { included, observed_at: snapshot.stamp.observed_at })
    }
}

impl SupplierCatalogItem {
    /// 更新独立地区事实，禁止迟到查询覆盖较新结果。
    /// # 参数
    /// `regions` 为已验证的完整地区观察。
    /// # 返回
    /// 保存来源地区，不改变报价、商品资料或人工配送模板。
    /// # 错误
    /// 观察时间倒退时拒绝。
    pub fn apply_service_regions(&mut self, regions: SourceServiceRegions) -> Result<()> {
        if self.source_service_regions.as_ref().is_some_and(|old| old.observed_at > regions.observed_at) {
            return Err(Error::ConflictError("已有更新的可售地区观察，请重新查询".into()));
        }
        self.source_service_regions = Some(regions);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::connector::common::{SourceRevision, SourceStamp};

    fn snapshot(value: RegionCoverage, at: i64) -> Snapshot<RegionCoverage> {
        Snapshot {
            value,
            stamp: SourceStamp {
                revision: SourceRevision::Unversioned,
                changed_at: None,
                observed_at: Instant::from_unix_secs(at),
            },
        }
    }

    #[test]
    fn service_regions_keep_empty_deduplicate_and_reject_unknown_or_old_evidence() {
        let received = Instant::from_unix_secs(10);
        let empty =
            SourceServiceRegions::queried(snapshot(RegionCoverage::Included(vec![]), 10), received).unwrap();
        assert!(empty.included.is_empty());
        let regions = SourceServiceRegions::queried(
            snapshot(RegionCoverage::Included(vec!["110100".into(), "110100".into()]), 11),
            received,
        )
        .unwrap();
        assert_eq!(regions.included, ["110100"]);

        for value in [
            RegionCoverage::Unreported,
            RegionCoverage::Unrestricted,
            RegionCoverage::Excluded(vec![]),
            RegionCoverage::Included(vec!["bad".into()]),
        ] {
            assert!(SourceServiceRegions::queried(snapshot(value, 11), received).is_err());
        }
        assert!(
            SourceServiceRegions::queried(snapshot(RegionCoverage::Included(vec![]), 9), received).is_err()
        );
    }
    #[test]
    fn catalog_sync_preserves_independent_regions_and_old_city_query_cannot_replace_them() {
        use crate::ports::connector::common::SupplierSku;
        use crate::ports::connector::offer::{Offer, OfferDetails};
        let mut item = SupplierCatalogItem::observed(
            "connection",
            "supplier",
            "job",
            Snapshot {
                stamp: SourceStamp {
                    revision: SourceRevision::Unversioned,
                    changed_at: None,
                    observed_at: Instant::from_unix_secs(10),
                },
                value: Offer {
                    sku: SupplierSku { product_id: Some("product".into()), spec_id: "spec".into() },
                    name: "商品".into(),
                    specification: "规格".into(),
                    unit: None,
                    quote: None,
                    details: OfferDetails::default(),
                },
            },
        )
        .unwrap();
        let mut ordinary_sync = item.clone();
        item.apply_service_regions(SourceServiceRegions {
            included: vec!["110100".into()],
            observed_at: Instant::from_unix_secs(20),
        })
        .unwrap();

        let mut historical = serde_json::to_value(&item).unwrap();
        historical.as_object_mut().unwrap().remove("source_service_regions");
        assert!(
            serde_json::from_value::<SupplierCatalogItem>(historical)
                .unwrap()
                .source_service_regions
                .is_none()
        );

        ordinary_sync.observed_at = Instant::from_unix_secs(30);
        ordinary_sync.merge_observation(item).unwrap();
        assert_eq!(ordinary_sync.source_service_regions.as_ref().unwrap().included, ["110100"]);

        assert!(
            ordinary_sync
                .apply_service_regions(SourceServiceRegions {
                    included: vec![],
                    observed_at: Instant::from_unix_secs(19)
                })
                .is_err()
        );
        assert_eq!(ordinary_sync.source_service_regions.as_ref().unwrap().included, ["110100"]);

        ordinary_sync
            .apply_service_regions(SourceServiceRegions {
                included: vec![],
                observed_at: Instant::from_unix_secs(31),
            })
            .unwrap();
        assert!(ordinary_sync.source_service_regions.unwrap().included.is_empty());
    }
}
