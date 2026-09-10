//! 自然日历周期，不包含任何业务付款政策。

use chrono::{Datelike, Days, NaiveDate};
use serde::{Deserialize, Serialize};

use super::time::BusinessDate;
use crate::{Error, Result};

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
    /// 计算日期所属自然周期的最后一天。
    ///
    /// # Errors
    /// 日期计算超出支持范围时返回错误。
    pub fn end(self, date: BusinessDate) -> Result<BusinessDate> {
        let date = date.as_naive_date();
        let end = self.end_date(date).ok_or("自然周期日期超出支持范围")?;
        BusinessDate::from_ymd(end.year(), end.month(), end.day())
            .ok_or_else(|| Error::from("自然周期日期无效"))
    }

    /// 通过下个周期首日减一天计算月度周期，自动处理闰年。
    fn end_date(self, date: NaiveDate) -> Option<NaiveDate> {
        if self == Self::Week {
            return date.checked_add_days(Days::new(6 - u64::from(date.weekday().num_days_from_monday())));
        }
        let months = match self {
            Self::Month => 1,
            Self::Quarter => 3,
            Self::HalfYear => 6,
            Self::Year => 12,
            Self::Week => unreachable!(),
        };
        let next_month = (date.month0() / months + 1) * months;
        let year = date.year().checked_add((next_month / 12) as i32)?;
        NaiveDate::from_ymd_opt(year, next_month % 12 + 1, 1)?.pred_opt()
    }
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
                period
                    .end(BusinessDate::from_ymd(date.0, date.1, date.2).unwrap())
                    .unwrap(),
                BusinessDate::from_ymd(expected.0, expected.1, expected.2).unwrap()
            );
        }
    }
}
