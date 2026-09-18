//! Support application services.

pub mod bulk_job;
pub mod file_asset;
pub mod source_registry;

use crate::error::{Error, Result};

/// 校验请求携带的期望版本与当前版本一致（乐观锁）。
///
/// 四个加载后校验点（快照/任务/资产/来源系统）共用同一冲突口径；
/// 不存在判定仍由调用方按各自领域文案返回 `NotFound`，本函数只承担
/// 版本一致性部分。
///
/// # 参数
/// * `actual` - 当前实体版本（`base.version`）
/// * `expected` - 请求携带的期望版本
///
/// # 返回
/// 一致时返回 `Ok(())`。
///
/// # 错误
/// 版本不一致时返回 `ConflictError`（调用方映射为 409）。
pub(crate) fn check_expected_version(actual: u64, expected: u64) -> Result<()> {
    if actual != expected {
        return Err(Error::ConflictError("数据已被其他请求修改，请刷新后重试".to_string()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::check_expected_version;

    #[test]
    fn version_check_accepts_matching_versions() {
        assert!(check_expected_version(3, 3).is_ok());
    }

    #[test]
    fn version_check_rejects_stale_versions() {
        assert!(check_expected_version(4, 3).is_err());
    }
}
