//! 目录检查点兼容、恢复与增量选择规则。
use super::supplier_catalog::{CATALOG_CHECKPOINT_VERSION, SupplierCatalogRun};
use crate::dto::supplier_catalog::CatalogSyncMode;
use crate::ports::catalog_sync::CatalogSyncPolicy;
use crate::ports::connector::offer::IncrementalSupport;
use crate::{Error, Result};

impl SupplierCatalogRun {
    /// 判断检查点格式及适配器版本能否由当前执行器继续处理。
    /// # 参数
    /// `config_version`、`mapping_version` 为当前连接及适配器版本。
    /// # 返回
    /// 当前格式和两个版本全部匹配时为 true。
    /// # 错误
    /// 不返回错误。
    pub fn compatible(&self, config_version: u64, mapping_version: u32) -> bool {
        self.checkpoint_version == CATALOG_CHECKPOINT_VERSION
            && self.technical_config_version == config_version
            && self.mapping_version == mapping_version
    }

    /// 仅恢复同连接、同技术配置及同映射版本的未完成扫描。
    /// # 参数
    /// `previous` 为调用方已核验属于最近失败任务的检查点。
    /// # 返回
    /// 兼容时沿用原窗口、游标、数量与异常；不兼容时保留新任务全量初态。
    /// # 错误
    /// 不返回错误。
    pub fn resume(&mut self, previous: &Self) {
        if previous.connection_id != self.connection_id
            || !previous.compatible(self.technical_config_version, self.mapping_version)
        {
            self.restart_required = true;
            return;
        }
        if previous.finished || previous.succeeded {
            return;
        }

        self.restart_required = previous.restart_required;
        self.scan_since = previous.scan_since;
        self.scan_through = previous.scan_through;
        self.sync_mode = previous.sync_mode;
        self.cursor = previous.cursor.clone();
        self.pages = previous.pages;
        self.observed_count = previous.observed_count;
        self.skipped_count = previous.skipped_count;
        self.skipped_samples = previous.skipped_samples.clone();
    }

    /// 标记扫描已成功结算；由调用方与任务成功状态同事务保存。
    /// # 参数
    /// 无。
    /// # 返回
    /// 完成扫描的检查点成为可查询的成功记录。
    /// # 错误
    /// 尚未提交末页时拒绝，不能制造不完整成功基线。
    pub fn complete(&mut self) -> Result<()> {
        if !self.finished {
            return Err(Error::ConflictError("目录扫描尚未完成，不能结算成功".into()));
        }
        self.succeeded = true;
        self.resume_requested = false;
        Ok(())
    }

    /// 在首轮读取前按适配器策略选择增量；恢复中的扫描保持原窗口。
    /// # 参数
    /// `latest` 为当前配置最近成功检查点，`full` 为最近无跳过项的成功全量检查点。
    /// # 返回
    /// 满足完整基线时设定增量窗口，其余保持全量。
    /// # 错误
    /// 不返回错误；非法周期或基线保守执行全量。
    pub fn plan(&mut self, policy: CatalogSyncPolicy, latest: Option<&Self>, full: Option<&Self>) {
        if self.restart_required
            || self.pages != 0
            || self.scan_since.is_some()
            || policy.incremental == IncrementalSupport::Unsupported
        {
            return;
        }

        let (Some(latest), Some(full)) = (latest, full) else { return };
        let Ok(interval) = i64::try_from(policy.full_sync_interval_seconds) else { return };
        if [latest, full].iter().any(|run| {
            run.connection_id != self.connection_id
                || !run.supports_incremental(self.technical_config_version, policy.mapping_version)
                || run.scan_through > self.scan_through
        }) || full.sync_mode != CatalogSyncMode::Full
            || full.scan_through.unix_secs() < self.scan_through.unix_secs().saturating_sub(interval)
        {
            return;
        }

        self.scan_since = Some(policy.since(latest.scan_through));
        self.sync_mode = match policy.incremental {
            IncrementalSupport::Complete => CatalogSyncMode::IncrementalComplete,
            _ => CatalogSyncMode::IncrementalPartial,
        };
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU32;

    use erp_core::common::time::Instant;

    use super::*;

    fn run() -> SupplierCatalogRun {
        serde_json::from_value(serde_json::json!({
            "id":"job", "created_at":0, "updated_at":0, "deleted_at":0, "version":0,
            "connection_id":"c", "technical_config_version":1, "mapping_version":7, "checkpoint_version":1,
            "cursor":null, "pages":0, "observed_count":0, "finished":false,
            "scan_through":100000
        }))
        .unwrap()
    }

    fn policy() -> CatalogSyncPolicy {
        CatalogSyncPolicy {
            mapping_version: 7,
            sync_interval_seconds: 60,
            full_sync_interval_seconds: 3600,
            overlap_seconds: 30,
            page_size: NonZeroU32::new(10).unwrap(),
            incremental: IncrementalSupport::Partial,
        }
    }

    fn completed() -> SupplierCatalogRun {
        let mut run = run();
        run.advance(None, 5).unwrap();
        run.succeeded = true;
        run.scan_through = Instant::from_unix_secs(99_000);
        run
    }

    #[test]
    fn recovery_preserves_window_and_progress_only_for_compatible_versions() {
        let mut previous = run();
        previous.advance(Some("8".into()), 35).unwrap();
        previous.scan_since = Some(Instant::from_unix_secs(90_000));
        previous.sync_mode = CatalogSyncMode::IncrementalPartial;
        previous.record_skipped(vec!["bad item".into()]).unwrap();

        let mut next = run();
        next.resume(&previous);
        assert_eq!(next.cursor.as_deref(), Some("8"));
        assert_eq!(next.observed_count, 35);
        assert_eq!(next.skipped_samples, previous.skipped_samples);
        assert_eq!(next.scan_since, previous.scan_since);
        assert_eq!(next.scan_through, previous.scan_through);

        for change in 0..6 {
            let mut incompatible = previous.clone();
            match change {
                0 => incompatible.mapping_version = 6,
                1 => incompatible.mapping_version = 0,
                2 => incompatible.technical_config_version = 2,
                3 => incompatible.connection_id = "other".into(),
                4 => incompatible.checkpoint_version = 0,
                _ => incompatible.finished = true,
            }
            let mut next = run();
            next.resume(&incompatible);
            assert_eq!((next.pages, next.cursor, next.scan_since), (0, None, None));
            assert_eq!(next.mapping_version, 7);
            assert_eq!(next.sync_mode, CatalogSyncMode::Full);
        }
    }

    #[test]
    fn incompatible_recovery_forces_full_even_when_older_compatible_baseline_exists() {
        let baseline = completed();
        let mut previous = run();
        previous.mapping_version = 8;
        previous.advance(Some("2".into()), 5).unwrap();

        let mut next = run();
        next.resume(&previous);
        next.plan(policy(), Some(&baseline), Some(&baseline));
        assert_eq!(next.sync_mode, CatalogSyncMode::Full);
        assert_eq!(next.scan_since, None);
        assert!(next.restart_required);
    }

    #[test]
    fn old_writer_dropping_checkpoint_format_cannot_be_resumed_or_used_as_baseline() {
        let mut old_writer = completed();
        let mut stored = serde_json::to_value(&old_writer).unwrap();
        stored.as_object_mut().unwrap().remove("checkpoint_version");
        old_writer = serde_json::from_value(stored).unwrap();
        assert!(!old_writer.supports_incremental(1, 7));

        old_writer.finished = false;
        old_writer.succeeded = false;
        old_writer.cursor = Some("9".into());
        let mut next = run();
        next.resume(&old_writer);
        assert!(next.restart_required);
        assert_eq!(next.cursor, None);
        assert_eq!(next.pages, 0);
    }

    #[test]
    fn completion_cannot_promote_an_unfinished_or_legacy_checkpoint() {
        let mut run = run();
        assert!(!run.succeeded);
        assert!(run.complete().is_err());

        run.advance(Some("2".into()), 5).unwrap();
        assert!(run.complete().is_err());

        run.advance(None, 0).unwrap();
        run.resume_requested = true;
        run.complete().unwrap();
        assert!(run.succeeded);
        assert!(!run.resume_requested);
    }

    #[test]
    fn unsupported_remains_full_and_complete_provider_uses_its_own_window() {
        let baseline = completed();
        let mut policy = policy();
        policy.incremental = IncrementalSupport::Unsupported;
        let mut next = run();
        next.plan(policy, Some(&baseline), Some(&baseline));
        assert_eq!(next.sync_mode, CatalogSyncMode::Full);
        assert_eq!(next.scan_since, None);

        policy.incremental = IncrementalSupport::Complete;
        policy.overlap_seconds = 0;
        next.plan(policy, Some(&baseline), Some(&baseline));
        assert_eq!(next.sync_mode, CatalogSyncMode::IncrementalComplete);
        assert_eq!(next.scan_since, Some(baseline.scan_through));
    }

    #[test]
    fn incremental_requires_recent_complete_success_and_keeps_recovery_window() {
        let full = completed();
        let mut latest = completed();
        latest.sync_mode = CatalogSyncMode::IncrementalPartial;
        latest.scan_through = Instant::from_unix_secs(99_900);

        let mut next = run();
        next.plan(policy(), Some(&latest), Some(&full));
        assert_eq!(next.scan_since.unwrap().unix_secs(), 99_870);
        assert_eq!(next.sync_mode, CatalogSyncMode::IncrementalPartial);

        let since = next.scan_since;
        next.plan(policy(), None, None);
        assert_eq!(next.scan_since, since);

        for change in 0..6 {
            let mut latest = latest.clone();
            let mut full = full.clone();
            match change {
                0 => latest.skipped_count = 1,
                1 => latest.succeeded = false,
                2 => latest.mapping_version = 6,
                3 => full.scan_through = Instant::from_unix_secs(90_000),
                4 => full.sync_mode = CatalogSyncMode::IncrementalPartial,
                _ => latest.scan_through = Instant::from_unix_secs(100_001),
            }
            let mut next = run();
            next.plan(policy(), Some(&latest), Some(&full));
            assert_eq!(next.scan_since, None);
            assert_eq!(next.sync_mode, CatalogSyncMode::Full);
        }
    }
}
