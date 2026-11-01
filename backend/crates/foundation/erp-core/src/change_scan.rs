//! 按 `updated_at` 扫描源数据变化的时间窗进度，供后台投影刷新共用。
//!
//! 即时扫描处理最新时间窗；延迟复扫在提交余量之后再处理一次同一时间窗，补上
//! 开始早、提交晚的事务写入。进度只前进不后退，多实例重复扫描只会重复标记。

use serde::{Deserialize, Serialize};

/// 事务开始到提交的最长间隔余量（秒）。
pub const COMMIT_MARGIN_SECS: i64 = 120;

/// 变更扫描的时间窗（Unix 秒，含起点不含终点）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChangeWindow {
    /// 起点（含）。
    pub from: i64,
    /// 终点（不含）。
    pub to: i64,
}

/// 即时扫描与延迟复扫的进度。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChangeWatermarks {
    /// 即时扫描已处理到的时间（Unix 秒，不含）。
    pub fresh_watermark: i64,
    /// 延迟复扫已处理到的时间（Unix 秒，不含）。
    pub settled_watermark: i64,
}

impl ChangeWatermarks {
    /// 全量重建开始时的进度：即时扫描从现在开始，延迟复扫回退一个提交余量。
    ///
    /// # 参数
    /// * `now` - 当前时间（Unix 秒）
    ///
    /// # 返回
    /// 返回起始进度。
    ///
    /// # 错误
    /// 无。
    pub fn starting_at(now: i64) -> Self {
        Self { fresh_watermark: now, settled_watermark: now - COMMIT_MARGIN_SECS }
    }

    /// 本轮需扫描的时间窗：即时扫描到现在，延迟复扫到现在减提交余量；空窗不返回。
    ///
    /// # 参数
    /// * `now` - 当前时间（Unix 秒）
    ///
    /// # 返回
    /// 返回非空时间窗。
    ///
    /// # 错误
    /// 无。
    pub fn scan_windows(&self, now: i64) -> Vec<ChangeWindow> {
        [
            ChangeWindow { from: self.fresh_watermark, to: now },
            ChangeWindow { from: self.settled_watermark, to: now - COMMIT_MARGIN_SECS },
        ]
        .into_iter()
        .filter(|window| window.from < window.to)
        .collect()
    }

    /// 读取前追平用的时间窗：包含当前这一秒，使刚提交的写入立即可见。
    ///
    /// `updated_at` 精确到秒；当前这一秒之后仍可能有写入，因此进度只推进到
    /// `now`（见 [`Self::fresh_advanced`]），这一秒会在下一次扫描中再扫一遍。
    ///
    /// # 参数
    /// * `now` - 当前时间（Unix 秒）
    ///
    /// # 返回
    /// 返回从即时扫描进度到当前这一秒（含）的时间窗。
    ///
    /// # 错误
    /// 无。
    pub fn catch_up_window(&self, now: i64) -> ChangeWindow {
        ChangeWindow { from: self.fresh_watermark.min(now), to: now + 1 }
    }

    /// 只推进即时扫描进度；延迟复扫仍按时间窗补漏。
    ///
    /// # 参数
    /// * `now` - 当前时间（Unix 秒）
    ///
    /// # 返回
    /// 返回即时扫描推进到 `now` 的进度。
    ///
    /// # 错误
    /// 无。
    pub fn fresh_advanced(&self, now: i64) -> Self {
        Self { fresh_watermark: self.fresh_watermark.max(now), ..*self }
    }

    /// 扫描完成后的进度；只前进不后退。
    ///
    /// # 参数
    /// * `now` - 本轮扫描使用的当前时间（Unix 秒）
    ///
    /// # 返回
    /// 返回推进后的进度。
    ///
    /// # 错误
    /// 无。
    pub fn advanced(&self, now: i64) -> Self {
        Self {
            fresh_watermark: self.fresh_watermark.max(now),
            settled_watermark: self.settled_watermark.max(now - COMMIT_MARGIN_SECS),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{COMMIT_MARGIN_SECS, ChangeWatermarks, ChangeWindow};

    #[test]
    fn settled_scan_replays_each_window_once_after_commit_margin() {
        let rebuilt_at = 1_000;
        let margin_start = rebuilt_at - COMMIT_MARGIN_SECS;
        let state = ChangeWatermarks::starting_at(rebuilt_at);
        assert_eq!(state.scan_windows(rebuilt_at), Vec::<ChangeWindow>::new());
        assert_eq!(
            state.scan_windows(1_002),
            vec![
                ChangeWindow { from: rebuilt_at, to: 1_002 },
                ChangeWindow { from: margin_start, to: margin_start + 2 },
            ]
        );
        let next = state.advanced(1_002);
        assert_eq!((next.fresh_watermark, next.settled_watermark), (1_002, margin_start + 2));
        assert_eq!(
            next.scan_windows(1_200),
            vec![
                ChangeWindow { from: 1_002, to: 1_200 },
                ChangeWindow { from: margin_start + 2, to: 1_200 - COMMIT_MARGIN_SECS },
            ]
        );
        let later = next.advanced(1_200);
        assert_eq!(later.settled_watermark, 1_200 - COMMIT_MARGIN_SECS);
        assert_eq!(later.advanced(1_100), later);
    }

    #[test]
    fn catch_up_includes_current_second_and_keeps_it_for_rescan() {
        let state = ChangeWatermarks::starting_at(1_000);
        assert_eq!(state.catch_up_window(1_000), ChangeWindow { from: 1_000, to: 1_001 });
        assert_eq!(state.catch_up_window(1_005), ChangeWindow { from: 1_000, to: 1_006 });
        let caught_up = state.fresh_advanced(1_005);
        assert_eq!(caught_up.fresh_watermark, 1_005);
        assert_eq!(caught_up.settled_watermark, state.settled_watermark);
        assert_eq!(caught_up.catch_up_window(1_005), ChangeWindow { from: 1_005, to: 1_006 });
        assert_eq!(caught_up.scan_windows(1_006)[0], ChangeWindow { from: 1_005, to: 1_006 });
    }
}
