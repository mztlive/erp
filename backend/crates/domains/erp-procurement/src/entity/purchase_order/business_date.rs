//! 销售履约期限与采购预计交付日的业务自然日换算。

use chrono::{Datelike, FixedOffset};
use erp_core::common::time::{BusinessDate, Instant};
use erp_core::{Error, Result};

/// 上海业务时区相对 UTC 的秒数。
const BUSINESS_TZ_OFFSET_SECS: i32 = 8 * 60 * 60;

/// 将精确时间转换为上海业务自然日。
///
/// # 参数
/// * `instant` - 销售履约期限
///
/// # 返回
/// 返回 Asia/Shanghai 自然日。
///
/// # 错误
/// 时区或日期构造失败时返回领域错误。
///
/// # 关键业务约束
/// 不按 UTC 日期截断。
pub fn business_date_of(instant: Instant) -> Result<BusinessDate> {
    let business_tz = FixedOffset::east_opt(BUSINESS_TZ_OFFSET_SECS)
        .ok_or_else(|| Error::from("无法形成 Asia/Shanghai 时区"))?;
    let naive = instant.as_utc().with_timezone(&business_tz).date_naive();
    BusinessDate::from_ymd(naive.year(), naive.month(), naive.day())
        .ok_or_else(|| Error::from("履约期限日期非法"))
}

#[cfg(test)]
mod tests {
    use erp_core::common::time::Instant;

    use super::business_date_of;

    /// UTC 16:00 落在上海次日零点。
    #[test]
    fn converts_to_shanghai_calendar_date() {
        let date = business_date_of(Instant::from_unix_secs(1_699_977_600)).unwrap();

        assert_eq!(date.to_string(), "2023-11-15");
    }
}
