//! 共享乐观锁版本校验（冲突返回 409）。
//!
//! 函数为 `pub`，crate 根不重导出。

use crate::{Result, optimistic_lock_conflict};

/// 校验期望乐观锁版本与当前版本一致（不一致返回 409）。
///
/// 一致返回 `Ok(())`。版本不一致时返回 `ConflictError`。
pub fn ensure_version(current_version: u64, expected_version: u64) -> Result<()> {
    if current_version != expected_version {
        return Err(optimistic_lock_conflict());
    }
    Ok(())
}
