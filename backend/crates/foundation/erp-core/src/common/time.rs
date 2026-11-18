//! `BusinessDate` / `Instant` 时间基元。
//!
//! - `BusinessDate`：业务自然日（`chrono::NaiveDate`），无时区语义，用于到期日、
//!   结算期间等只关心自然日的字段；serde 形态为 `YYYY-MM-DD` 字符串。
//! - `Instant`：统一时基时间点（`chrono::DateTime<Utc>`），持久化统一时基，
//!   展示层按业务时区转换；serde 形态为秒级时间戳（i64），与
//!   `entity_core::BaseModel.created_at`（u64 秒）的 JSON 数字形态一致。

use std::fmt;
use std::str::FromStr;

use chrono::{DateTime, Datelike, FixedOffset, NaiveDate, Utc};
use serde::de::Visitor;
use serde::{Deserialize, Deserializer, Serialize, Serializer, de};

use crate::errors::{Error, Result};

/// 业务时区相对 UTC 的偏移秒数（Asia/Shanghai，+08:00）。
/// 业务自然日、前台日期归属与展示转换共用同一偏移，禁止在函数内重复手写 `8 * 3600`。
pub const BUSINESS_TZ_OFFSET_SECS: i32 = 8 * 3600;

/// 业务自然日（无时区语义）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BusinessDate(NaiveDate);

impl BusinessDate {
    /// 把表单或自然历计算得到的年月日构造成无时区的业务日，适用于到期日等字段；不存在的日期返回 `None`。
    pub fn from_ymd(year: i32, month: u32, day: u32) -> Option<Self> {
        NaiveDate::from_ymd_opt(year, month, day).map(Self)
    }

    // 将日历运算已验证的 `NaiveDate` 封装回业务日期，不再转换时区或重复校验。
    pub(crate) fn from_naive_date(date: NaiveDate) -> Self {
        Self(date)
    }

    /// 按上海业务时区（UTC+08:00）取当天日期，用于客户、合同等按自然日判断的查询。
    /// 否则当地凌晨至 08:00 直接取 UTC 日期会落到前一日，可能漏掉当天新数据。
    ///
    /// # Panics
    /// 仅当共享固定偏移常量不在 `FixedOffset` 支持范围内时触发；当前 +08:00 合法。
    pub fn today() -> Self {
        let business_tz = FixedOffset::east_opt(BUSINESS_TZ_OFFSET_SECS).expect("+08:00 固定偏移量必然合法");
        Self(Utc::now().with_timezone(&business_tz).date_naive())
    }

    /// 在计算周期或比较日历边界时取底层自然日，不添加时间或时区。
    pub fn as_naive_date(self) -> NaiveDate {
        self.0
    }

    /// 查询合同等按日生效的对象时判断有效期。包含生效日，排除失效日；没有失效日则不限制上界。
    pub fn is_within(self, valid_from: BusinessDate, valid_to: Option<BusinessDate>) -> bool {
        valid_from <= self && valid_to.is_none_or(|until| self < until)
    }

    /// 拆出年、月、日，供需要按自然历分量处理业务日期的调用方使用。
    pub fn ymd(self) -> (i32, u32, u32) {
        (self.0.year(), self.0.month(), self.0.day())
    }
}

impl fmt::Display for BusinessDate {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.0.format("%Y-%m-%d"))
    }
}

impl FromStr for BusinessDate {
    type Err = Error;

    // 解析表单或外部文本中的业务日期；展示使用 `YYYY-MM-DD`，非法格式及不存在的日期归为业务值错误。
    fn from_str(value: &str) -> Result<Self> {
        let date = NaiveDate::parse_from_str(value, "%Y-%m-%d")
            .map_err(|_| Error::from("请填写有效日期，例如 2026-08-05"))?;
        Ok(Self(date))
    }
}

impl Serialize for BusinessDate {
    // 将业务日写成固定的 `YYYY-MM-DD` 字符串，避免给只关心自然日的字段引入时区。
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for BusinessDate {
    // 回读接口或持久化业务日期时只接受字符串；日期交由 `BusinessDate::from_str` 检查，保留同一错误提示。
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        Self::from_str(&value).map_err(de::Error::custom)
    }
}

/// 统一时基时间点（UTC 秒级精度）。
///
/// 业务发生时间 `occurred_at` 与记录时间 `recorded_at` 一律使用本类型持久化，
/// 页面展示时再按业务时区转换。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Instant(DateTime<Utc>);

impl Instant {
    /// 捕获当前 UTC 时刻，供记录业务事实在 ERP 中的登记时间等需要真实时刻的场景使用。
    pub fn now() -> Self {
        Self(Utc::now())
    }

    /// 用已知可表示的 Unix 秒构造业务时刻，负数表示 1970-01-01 UTC 纪元起点之前。
    /// 外部或未验证输入应使用 [`Instant::try_from_unix_secs`]。
    ///
    /// # Panics
    /// 秒数超过 `DateTime<Utc>` 可表示的范围时触发 panic。
    pub fn from_unix_secs(secs: i64) -> Self {
        Self::try_from_unix_secs(secs).expect("Unix 时间戳超出可表示范围")
    }

    /// 转换外部或回读的 Unix 秒为 UTC 时刻，支持负值；超出 `DateTime<Utc>` 范围时返回业务值错误而非 panic。
    pub fn try_from_unix_secs(secs: i64) -> Result<Self> {
        DateTime::from_timestamp(secs, 0).map(Self).ok_or_else(|| Error::from("Unix 时间戳超出可表示范围"))
    }

    /// 取与持久化合同一致的有符号 Unix 整秒，用于存储或跨层传输。
    pub fn unix_secs(self) -> i64 {
        self.0.timestamp()
    }

    /// 取底层 UTC 时刻，供展示层转换到业务时区或供调用方执行时间运算。
    pub fn as_utc(self) -> DateTime<Utc> {
        self.0
    }
}

impl Serialize for Instant {
    // 持久化记录时间时写为有符号 Unix 整秒；JSON 保持与 `BaseModel.created_at` 一致的数字形态。
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        serializer.serialize_i64(self.unix_secs())
    }
}

impl<'de> Deserialize<'de> for Instant {
    // 回读时接受有符号或无符号的 32/64 位整数秒；拒绝浮点、非整数类型和无法表达的时刻。
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        struct InstantVisitor;

        impl<'de> Visitor<'de> for InstantVisitor {
            type Value = Instant;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("秒级 Unix 时间戳（整数）")
            }

            // 解码有符号 Unix 秒时复用时刻范围检查，把不可表示的时间归为 serde 错误。
            fn visit_i64<E: de::Error>(self, value: i64) -> std::result::Result<Self::Value, E> {
                Instant::try_from_unix_secs(value).map_err(de::Error::custom)
            }

            // 解码无符号 Unix 秒时先检查能否转入 `i64`，再校验时刻范围，避免溢出后变成负数。
            fn visit_u64<E: de::Error>(self, value: u64) -> std::result::Result<Self::Value, E> {
                let secs =
                    i64::try_from(value).map_err(|_| de::Error::custom("时间戳超出 i64 可表示范围"))?;
                self.visit_i64(secs)
            }

            fn visit_i32<E: de::Error>(self, value: i32) -> std::result::Result<Self::Value, E> {
                self.visit_i64(i64::from(value))
            }

            fn visit_u32<E: de::Error>(self, value: u32) -> std::result::Result<Self::Value, E> {
                self.visit_u64(u64::from(value))
            }
        }

        deserializer.deserialize_any(InstantVisitor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // 核对有效日期可构造和回读、展示始终补零，并拒绝日历中不存在的日期。
    #[test]
    fn business_date_roundtrip() {
        let date = BusinessDate::from_ymd(2026, 8, 5).unwrap();
        assert_eq!(date.to_string(), "2026-08-05");
        assert_eq!(date.ymd(), (2026, 8, 5));
        assert!(BusinessDate::from_ymd(2026, 2, 30).is_none(), "2 月 30 日不存在");

        let parsed: BusinessDate = "2026-08-05".parse().unwrap();
        assert_eq!(parsed, date);
        // chrono 解析对零填充宽松：非零填充输入也可解析，展示恒为 YYYY-MM-DD
        let lenient: BusinessDate = "2026-8-5".parse().unwrap();
        assert_eq!(lenient.to_string(), "2026-08-05");
        assert!("2026-08-32".parse::<BusinessDate>().is_err());
    }

    // 锁定接口日期的字符串表示和往返行为，防止误接受斜杠分隔格式。
    #[test]
    fn business_date_serde_shape() {
        let date = BusinessDate::from_ymd(2026, 8, 5).unwrap();
        assert_eq!(serde_json::to_string(&date).unwrap(), "\"2026-08-05\"");
        let back: BusinessDate = serde_json::from_str("\"2026-08-05\"").unwrap();
        assert_eq!(back, date);
        assert!(serde_json::from_str::<BusinessDate>("\"2026/08/05\"").is_err());
    }

    // 核对 JSON 使用整数 Unix 秒并可回读，兼顾纪元前的负值与禁止浮点的约束。
    #[test]
    fn instant_secs_roundtrip() {
        let instant = Instant::from_unix_secs(1_700_000_000);
        assert_eq!(instant.unix_secs(), 1_700_000_000);
        assert_eq!(Instant::from_unix_secs(-1).unix_secs(), -1);

        let json = serde_json::to_string(&instant).unwrap();
        assert_eq!(json, "1700000000");
        let back: Instant = serde_json::from_str(&json).unwrap();
        assert_eq!(back, instant);

        let from_u64: Instant = serde_json::from_str("1700000000").unwrap();
        assert_eq!(from_u64, instant);

        assert!(serde_json::from_str::<Instant>("1700000000.5").is_err(), "禁止浮点");
    }

    // 锁定记录时间在 MongoDB 中使用 BSON Int64 而非日期对象，并验证文档回读不变。
    #[test]
    fn instant_bson_roundtrip() {
        #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
        struct Doc {
            at: Instant,
        }

        let doc = Doc { at: Instant::from_unix_secs(1_700_000_000) };
        let bson_doc = bson::serialize_to_document(&doc).unwrap();
        assert!(matches!(bson_doc.get("at"), Some(bson::Bson::Int64(_))));
        let back: Doc = bson::deserialize_from_document(bson_doc).unwrap();
        assert_eq!(back, doc);
    }

    // 核对用于时间排序的大小比较与先后顺序一致，同一秒构造的值相等。
    #[test]
    fn instant_ordering() {
        let earlier = Instant::from_unix_secs(100);
        let later = Instant::from_unix_secs(200);
        assert!(earlier < later);
        assert!(later > earlier);
        assert_eq!(earlier, Instant::from_unix_secs(100));
    }

    #[test]
    fn validity_window_includes_start_and_excludes_end() {
        let date = |value: &str| value.parse::<BusinessDate>().unwrap();
        let start = date("2026-10-01");
        let end = Some(date("2026-10-16"));
        assert!(date("2026-10-01").is_within(start, end));
        assert!(date("2026-10-15").is_within(start, end));
        assert!(!date("2026-10-16").is_within(start, end));
        assert!(!date("2026-09-30").is_within(start, end));
        assert!(date("2099-01-01").is_within(start, None));
    }
}

#[cfg(test)]
mod instant_boundary_tests {
    use super::Instant;
    #[test]
    fn decoding_out_of_range_timestamps_returns_errors() {
        for raw in [i64::MIN.to_string(), i64::MAX.to_string(), u64::MAX.to_string()] {
            assert!(serde_json::from_str::<Instant>(&raw).is_err());
        }
        for seconds in [-1, 0, 1, 1_700_000_000] {
            assert_eq!(serde_json::from_str::<Instant>(&seconds.to_string()).unwrap().unix_secs(), seconds);
        }
    }
}
