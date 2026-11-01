//! 供应商目录采集策略；协议差异由适配器声明，共用执行器只消费策略。
use std::num::NonZeroU32;

use erp_core::common::time::Instant;

use super::connector::offer::IncrementalSupport;

/// 当前适配器的目录协议和调度策略；映射或游标语义改变时须递增版本。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CatalogSyncPolicy {
    pub mapping_version: u32,
    pub sync_interval_seconds: u64,
    pub full_sync_interval_seconds: u64,
    pub overlap_seconds: u32,
    pub page_size: NonZeroU32,
    pub incremental: IncrementalSupport,
}

impl CatalogSyncPolicy {
    /// 按协议要求计算下一次增量的重叠起点。
    /// # 参数
    /// `through` 为上次成功扫描冻结的截止时间。
    /// # 返回
    /// 回退协议要求秒数后的时间；零重叠保持原值。
    /// # 错误
    /// 不返回错误，时间下溢饱和处理。
    pub fn since(self, through: Instant) -> Instant {
        Instant::from_unix_secs(through.unix_secs().saturating_sub(i64::from(self.overlap_seconds)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overlap_is_a_provider_policy_not_a_fixed_day() {
        let mut policy = CatalogSyncPolicy {
            mapping_version: 1,
            sync_interval_seconds: 60,
            full_sync_interval_seconds: 3600,
            overlap_seconds: 0,
            page_size: NonZeroU32::new(20).unwrap(),
            incremental: IncrementalSupport::Complete,
        };
        let through = Instant::from_unix_secs(100_000);
        assert_eq!(policy.since(through), through);
        policy.overlap_seconds = 60;
        assert_eq!(policy.since(through).unix_secs(), 99_940);
        policy.overlap_seconds = 86_400;
        assert_eq!(policy.since(through).unix_secs(), 13_600);
    }
}
