//! 共享乐观锁版本校验（冲突返回 409）。
//!
//! 仅限本域内部调用（`pub`），不对外暴露。

use crate::{Result, optimistic_lock_conflict};

/// 校验期望乐观锁版本与当前版本一致（不一致返回 409）。
///
/// # 参数
/// * `current_version` - 当前版本
/// * `expected_version` - 请求携带的期望版本
///
/// # 返回
/// 一致返回 `Ok(())`。
///
/// # 错误
/// 版本不一致时返回 `ConflictError`。
pub fn ensure_version(current_version: u64, expected_version: u64) -> Result<()> {
    if current_version != expected_version {
        return Err(optimistic_lock_conflict());
    }
    Ok(())
}
