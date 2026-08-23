//! `BusinessDate` / `Instant` 时间基元（P0-1.4 共享基元任务）。
//!
//! - `BusinessDate`：业务自然日（`chrono::NaiveDate`），无时区语义，用于到期日、
//!   结算期间等只关心自然日的字段；serde 形态为 `YYYY-MM-DD` 字符串。
//! - `Instant`：统一时基时间点（`chrono::DateTime<Utc>`），持久化统一时基，
//!   展示层按业务时区转换；serde 形态为秒级时间戳（i64），与
//!   `entity_core::BaseModel.created_at`（u64 秒）的 JSON 数字形态一致。

use std::fmt;
use std::str::FromStr;

use chrono::{DateTime, Datelike, FixedOffset, NaiveDate, Utc};
use serde::{de, de::Visitor, Deserialize, Deserializer, Serialize, Serializer};

use crate::errors::{Error, Result};

/// 业务自然日（无时区语义）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BusinessDate(NaiveDate);

impl BusinessDate {
    /// 按年/月/日构造业务日期。
    ///
    /// # 参数
    /// * `year` - 年份
    /// * `month` - 月份（1–12）
    /// * `day` - 日（1–31）
    ///
    /// # 返回
    /// 日期合法返回 `Some`，非法（如 2 月 30 日）返回 `None`。
    pub fn from_ymd(year: i32, month: u32, day: u32) -> Option<Self> {
        NaiveDate::from_ymd_opt(year, month, day).map(Self)
    }

    /// 返回今天的业务自然日。
    ///
    /// # 返回
    /// 以业务时区（Asia/Shanghai，+08:00）计算的今天。业务日期无时区语义，
    /// 前端与各类业务输入（客户/合同/单据的生效日期、到期日）均按中国业务
    /// 时区的自然日构造，这里必须与之一致，否则 00:00–08:00 期间以 UTC 计算
    /// 的“今天”会早于前端日期，导致归属范围查询（如客户列表）查不到当天
    /// 新建的数据。
    pub fn today() -> Self {
        const BUSINESS_UTC_OFFSET_SECS: i32 = 8 * 3600;
        let business_tz = FixedOffset::east_opt(BUSINESS_UTC_OFFSET_SECS).expect("+08:00 固定偏移量必然合法");
        Self(Utc::now().with_timezone(&business_tz).date_naive())
    }

    /// 返回底层 `NaiveDate`。
    ///
    /// # 返回
    /// 返回内部日期值。
    pub fn as_naive_date(self) -> NaiveDate {
        self.0
    }

    /// 拆分为年/月/日。
    ///
    /// # 返回
    /// 返回 `(year, month, day)` 元组。
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

    /// 从 `YYYY-MM-DD` 字符串解析业务日期。
    ///
    /// # 参数
    /// * `value` - 日期字符串（须为零填充，如 `2026-08-05`）
    ///
    /// # 返回
    /// 解析成功返回 `Ok`，否则返回 `Err`。
    ///
    /// # 错误
    /// 格式非法或日期不存在（如 2 月 30 日）时返回 `LogicError`。
    fn from_str(value: &str) -> Result<Self> {
        let date = NaiveDate::parse_from_str(value, "%Y-%m-%d")
            .map_err(|_| Error::from(format!("{value} 不是合法业务日期（期望 YYYY-MM-DD）")))?;
        Ok(Self(date))
    }
}

impl Serialize for BusinessDate {
    /// 序列化为 `YYYY-MM-DD` 字符串。
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for BusinessDate {
    /// 从 `YYYY-MM-DD` 字符串反序列化。
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
    /// 返回当前时刻。
    ///
    /// # 返回
    /// 返回 UTC 当前时刻。
    pub fn now() -> Self {
        Self(Utc::now())
    }

    /// 由 Unix 秒级时间戳构造时刻。
    ///
    /// # 参数
    /// * `secs` - Unix 秒（可为负数，表示 1970 年以前）
    ///
    /// # 返回
    /// 返回对应时刻。
    ///
    /// # Panics
    /// 时间戳超出 `DateTime<Utc>` 可表示范围时 panic（i64 秒在 ±2920 亿年范围
    /// 内，实际业务数据不会触达）。
    pub fn from_unix_secs(secs: i64) -> Self {
        Self(DateTime::from_timestamp(secs, 0).expect("Unix 时间戳超出可表示范围"))
    }

    /// 返回 Unix 秒级时间戳。
    ///
    /// # 返回
    /// 返回整秒时间戳（i64）。
    pub fn unix_secs(self) -> i64 {
        self.0.timestamp()
    }

    /// 返回底层 `DateTime<Utc>`。
    ///
    /// # 返回
    /// 返回内部时刻值。
    pub fn as_utc(self) -> DateTime<Utc> {
        self.0
    }
}

impl Serialize for Instant {
    /// 序列化为秒级时间戳（i64 数字，与 `BaseModel.created_at` 的 JSON 形态一致）。
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        serializer.serialize_i64(self.unix_secs())
    }
}

impl<'de> Deserialize<'de> for Instant {
    /// 从秒级时间戳反序列化（接受 i64/u64/i32/u32，拒绝浮点）。
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        struct InstantVisitor;

        impl<'de> Visitor<'de> for InstantVisitor {
            type Value = Instant;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("秒级 Unix 时间戳（整数）")
            }

            fn visit_i64<E: de::Error>(self, value: i64) -> std::result::Result<Self::Value, E> {
                Ok(Instant::from_unix_secs(value))
            }

            fn visit_u64<E: de::Error>(self, value: u64) -> std::result::Result<Self::Value, E> {
                let secs =
                    i64::try_from(value).map_err(|_| de::Error::custom("时间戳超出 i64 可表示范围"))?;
                Ok(Instant::from_unix_secs(secs))
            }

            fn visit_i32<E: de::Error>(self, value: i32) -> std::result::Result<Self::Value, E> {
                self.visit_i64(value as i64)
            }

            fn visit_u32<E: de::Error>(self, value: u32) -> std::result::Result<Self::Value, E> {
                self.visit_u64(value as u64)
            }
        }

        deserializer.deserialize_any(InstantVisitor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// BusinessDate 构造、显示与解析往返。
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

    /// BusinessDate 的 serde 字符串形态。
    #[test]
    fn business_date_serde_shape() {
        let date = BusinessDate::from_ymd(2026, 8, 5).unwrap();
        assert_eq!(serde_json::to_string(&date).unwrap(), "\"2026-08-05\"");
        let back: BusinessDate = serde_json::from_str("\"2026-08-05\"").unwrap();
        assert_eq!(back, date);
        assert!(serde_json::from_str::<BusinessDate>("\"2026/08/05\"").is_err());
    }

    /// Instant 秒级时间戳往返（JSON 数字形态，与 BaseModel.created_at 一致）。
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

        assert!(
            serde_json::from_str::<Instant>("1700000000.5").is_err(),
            "禁止浮点"
        );
    }

    /// Instant 的 BSON 形态为 Int64 且可往返。
    #[test]
    fn instant_bson_roundtrip() {
        #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
        struct Doc {
            at: Instant,
        }

        let doc = Doc {
            at: Instant::from_unix_secs(1_700_000_000),
        };
        let bson_doc = bson::to_document(&doc).unwrap();
        assert!(matches!(bson_doc.get("at"), Some(bson::Bson::Int64(_))));
        let back: Doc = bson::from_document(bson_doc).unwrap();
        assert_eq!(back, doc);
    }

    /// 时间先后比较。
    #[test]
    fn instant_ordering() {
        let earlier = Instant::from_unix_secs(100);
        let later = Instant::from_unix_secs(200);
        assert!(earlier < later);
        assert!(later > earlier);
        assert_eq!(earlier, Instant::from_unix_secs(100));
    }
}
