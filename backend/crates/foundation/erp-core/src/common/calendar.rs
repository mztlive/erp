//! 自然日历周期，不包含任何业务付款政策。

use chrono::{Datelike, Days, NaiveDate};
use serde::{Deserialize, Serialize};

use super::time::BusinessDate;
use crate::Result;

/// 周一开始的自然周，以及自然月、季度、半年、年。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CalendarPeriod {
    Week,
    Month,
    Quarter,
    HalfYear,
    Year,
}

impl CalendarPeriod {
    /// 从周期内的业务日期求自然周期最后一天，供需要日历边界的领域计算使用，不引入付款政策。
    /// 周从周一算起，季度和半年从 1 月分段；日期运算越界返回业务值错误。
    pub fn end(self, date: BusinessDate) -> Result<BusinessDate> {
        let date = date.as_naive_date();
        let end = self.end_date(date).ok_or("自然周期日期超出支持范围")?;
        Ok(BusinessDate::from_naive_date(end))
    }

    // 在封装业务日期前按周期计算终日；自然周求周日，其他周期用月边界，溢出时留给上层统一报错。
    fn end_date(self, date: NaiveDate) -> Option<NaiveDate> {
        match self {
            Self::Week => {
                date.checked_add_days(Days::new(6 - u64::from(date.weekday().num_days_from_monday())))
            },
            Self::Month => month_end(date, 1),
            Self::Quarter => month_end(date, 3),
            Self::HalfYear => month_end(date, 6),
            Self::Year => month_end(date, 12),
        }
    }
}

// 按每周期月数找到下一周期首日，再退一天，避免为月长和闰年另列规则。
fn month_end(date: NaiveDate, months: u32) -> Option<NaiveDate> {
    let next_month = (date.month0() / months + 1) * months;
    let year = date.year().checked_add((next_month / 12) as i32)?;
    NaiveDate::from_ymd_opt(year, next_month % 12 + 1, 1)?.pred_opt()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn natural_periods_include_leap_days_and_cross_year_weeks() {
        for (period, date, expected) in [
            (CalendarPeriod::Month, (2028, 2, 10), (2028, 2, 29)),
            (CalendarPeriod::Quarter, (2026, 9, 10), (2026, 9, 30)),
            (CalendarPeriod::HalfYear, (2026, 1, 10), (2026, 6, 30)),
            (CalendarPeriod::HalfYear, (2026, 9, 10), (2026, 12, 31)),
            (CalendarPeriod::Year, (2026, 9, 10), (2026, 12, 31)),
            (CalendarPeriod::Week, (2026, 12, 31), (2027, 1, 3)),
        ] {
            assert_eq!(
                period.end(BusinessDate::from_ymd(date.0, date.1, date.2).unwrap()).unwrap(),
                BusinessDate::from_ymd(expected.0, expected.1, expected.2).unwrap()
            );
        }
    }
}
