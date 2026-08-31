//! 工作项到期筛选的统一业务时区窗口。

use chrono::{Datelike, FixedOffset, TimeZone};
use serde::{Deserialize, Serialize};

use crate::common::time::Instant;
use crate::errors::{Error, Result};

/// 工作项到期筛选。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum WorkItemDueFilter {
    /// Asia/Shanghai 当日 `[00:00, 次日 00:00)`。
    Today,
    /// 严格早于当前时点。
    Overdue,
}

/// 到期查询的半开窗口。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorkItemDueWindow {
    /// 包含的下界；无下界表示从最早时点开始。
    pub from: Option<Instant>,
    /// 不包含的上界。
    pub before: Instant,
}

impl WorkItemDueFilter {
    /// 返回稳定筛选代码。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Today => "today",
            Self::Overdue => "overdue",
        }
    }

    /// 在固定 Asia/Shanghai 业务时区形成查询窗口。
    ///
    /// `Overdue` 的上界为 `now`，因此 `due_at == now` 不逾期；`Today` 与
    /// `Overdue` 可在今日已经过去的时段重叠。
    ///
    /// # 错误
    /// 传入时间无法转换为业务日边界时返回错误。
    pub fn window_at(self, now: Instant) -> Result<WorkItemDueWindow> {
        let timezone =
            FixedOffset::east_opt(8 * 60 * 60).ok_or_else(|| Error::from("无法形成 Asia/Shanghai 时区"))?;
        let local = timezone
            .timestamp_opt(now.unix_secs(), 0)
            .single()
            .ok_or_else(|| Error::from("无法读取统计时点"))?;
        let start = timezone
            .with_ymd_and_hms(local.year(), local.month(), local.day(), 0, 0, 0)
            .single()
            .ok_or_else(|| Error::from("无法形成业务日边界"))?;
        let start = Instant::from_unix_secs(start.timestamp());
        let tomorrow = Instant::from_unix_secs(start.unix_secs() + 86_400);
        Ok(match self {
            Self::Today => WorkItemDueWindow {
                from: Some(start),
                before: tomorrow,
            },
            Self::Overdue => WorkItemDueWindow {
                from: None,
                before: now,
            },
        })
    }

    /// 按单值展示优先级返回指定到期时点的分类。
    ///
    /// # 返回
    /// 逾期优先于今日；不属于任一分类时返回 `None`。
    pub fn display_at(due_at: Instant, now: Instant) -> Result<Option<Self>> {
        if due_at.unix_secs() < now.unix_secs() {
            return Ok(Some(Self::Overdue));
        }
        let today = Self::Today.window_at(now)?;
        Ok(
            (due_at.unix_secs() >= today.from.expect("今日窗口必须有下界").unix_secs()
                && due_at.unix_secs() < today.before.unix_secs())
            .then_some(Self::Today),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::WorkItemDueFilter;
    use crate::common::time::Instant;

    #[test]
    fn shanghai_windows_keep_now_exclusive_and_today_overlapping() {
        let now = Instant::from_unix_secs(1_788_143_700); // 2026-08-31T10:35:00+08:00
        let today = WorkItemDueFilter::Today.window_at(now).unwrap();
        let overdue = WorkItemDueFilter::Overdue.window_at(now).unwrap();
        assert_eq!(today.from.unwrap().unix_secs(), 1_788_105_600);
        assert_eq!(today.before.unix_secs(), 1_788_192_000);
        assert_eq!(overdue.before, now);
        assert_eq!(
            WorkItemDueFilter::display_at(now, now).unwrap(),
            Some(WorkItemDueFilter::Today)
        );
        assert_eq!(
            WorkItemDueFilter::display_at(Instant::from_unix_secs(now.unix_secs() - 1), now).unwrap(),
            Some(WorkItemDueFilter::Overdue)
        );
        assert_eq!(
            WorkItemDueFilter::display_at(Instant::from_unix_secs(1_788_105_600), now).unwrap(),
            Some(WorkItemDueFilter::Overdue)
        );
        assert_eq!(
            WorkItemDueFilter::display_at(Instant::from_unix_secs(1_788_019_200), now).unwrap(),
            Some(WorkItemDueFilter::Overdue)
        );
        assert_eq!(
            WorkItemDueFilter::display_at(Instant::from_unix_secs(1_788_192_000), now).unwrap(),
            None
        );
    }
}
