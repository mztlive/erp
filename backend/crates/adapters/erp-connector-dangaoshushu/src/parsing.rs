use std::str::FromStr;

use chrono::{FixedOffset, NaiveDateTime, TimeZone};
use erp_core::common::time::{BUSINESS_TZ_OFFSET_SECS, Instant};
use erp_core::money::Amount;
use erp_supply::entity::failure::SupplierFailureClass;
use erp_supply::ports::connector::common::{
    ConnectorError, ConnectorResult, Snapshot, SourceRevision, SourceStamp,
};
use erp_supply::ports::connector::order::OrderLine;
use serde_json::Value;

use super::error;

pub(super) fn delivery_expired() -> ConnectorError {
    error(SupplierFailureClass::BusinessRejected, "DGSS_DELIVERY_EXPIRED", "配送选项已过期，须重新查询后提交")
}

pub(super) fn mapping() -> ConnectorError {
    error(
        SupplierFailureClass::MappingError,
        "DGSS_MAPPING",
        "蛋糕叔叔提供的业务资料不完整或无法核实，请联系管理员核对供应商资料",
    )
}
// 目录字段解析失败时补上固定的字段说明，便于定位供应商资料问题；不回显原值，也不改写非映射错误。
pub(super) fn context(mut failure: ConnectorError, message: &str) -> ConnectorError {
    if failure.class == SupplierFailureClass::MappingError {
        failure.summary = format!("{message}，请核对蛋糕叔叔的商品资料");
    }
    failure
}
pub(super) fn text(value: &Value) -> ConnectorResult<String> {
    match value {
        Value::String(value) if !value.trim().is_empty() && value.trim() == value => Ok(value.clone()),
        Value::Number(value) => Ok(value.to_string()),
        _ => Err(mapping()),
    }
}
pub(super) fn field(value: &Value, key: &str) -> ConnectorResult<String> {
    text(&value[key])
}

pub(super) fn id(value: &Value) -> ConnectorResult<String> {
    let identifier = text(value)?;
    if value.is_number() && !identifier.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(mapping());
    }
    valid_id(&identifier)?;
    Ok(identifier)
}
pub(super) fn valid_id(value: &str) -> ConnectorResult<()> {
    if value.is_empty()
        || value.len() > 128
        || !value.bytes().all(|byte| byte.is_ascii_alphanumeric() || b"-_.".contains(&byte))
    {
        return Err(mapping());
    }
    Ok(())
}
pub(super) fn flag(value: &Value) -> ConnectorResult<bool> {
    match text(value)?.as_str() {
        "0" => Ok(false),
        "1" => Ok(true),
        _ => Err(mapping()),
    }
}
pub(super) fn fixed<T: FromStr>(value: &Value) -> ConnectorResult<T> {
    text(value)?.parse().map_err(|_| mapping())
}

// 配送费和订单应付金额入业务结果前按十进制精确解析，允许零元；缺值、负数或格式及精度非法均返回映射错误。
pub(super) fn non_negative_amount(value: &Value) -> ConnectorResult<Amount> {
    let amount: Amount = fixed(value)?;
    if amount.to_decimal().is_sign_negative() {
        return Err(mapping());
    }
    Ok(amount)
}

pub(super) fn join_field<'a>(lines: &'a [OrderLine], f: impl Fn(&'a OrderLine) -> &'a str) -> String {
    lines.iter().map(f).collect::<Vec<_>>().join(",")
}

pub(super) fn join_quantities(lines: &[OrderLine]) -> String {
    lines.iter().map(|line| line.quantity.to_decimal().normalize().to_string()).collect::<Vec<_>>().join(",")
}

pub(super) fn business_tz() -> FixedOffset {
    FixedOffset::east_opt(BUSINESS_TZ_OFFSET_SECS).expect("业务时区固定为 +08:00")
}

pub(super) fn source_time(value: &Value) -> ConnectorResult<Option<Instant>> {
    if value.is_null() || value.as_str() == Some("") {
        return Ok(None);
    }
    let text = text(value)?;
    let date = NaiveDateTime::parse_from_str(&text, "%Y-%m-%d %H:%M:%S").map_err(|_| mapping())?;
    let date = business_tz().from_local_datetime(&date).single().ok_or_else(mapping)?;
    Ok(Some(Instant::from_unix_secs(date.timestamp())))
}
pub(super) fn snapshot<T>(value: T, changed_at: Option<Instant>) -> Snapshot<T> {
    Snapshot {
        value,
        stamp: SourceStamp { revision: SourceRevision::Unversioned, changed_at, observed_at: Instant::now() },
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn non_negative_amount_preserves_scale_and_rejects_invalid_values() {
        for text in ["0", "0.00", "12.30", "79228162514264337593543950335"] {
            assert_eq!(non_negative_amount(&json!(text)).unwrap().to_string(), text);
        }
        assert_eq!(non_negative_amount(&json!(123)).unwrap().to_string(), "123");

        for value in [
            Value::Null,
            json!(true),
            json!([]),
            json!("-0.01"),
            json!("1.001"),
            json!("invalid"),
            json!(""),
            json!(" 1 "),
            json!("79228162514264337593543950336"),
        ] {
            assert_eq!(non_negative_amount(&value).unwrap_err(), mapping());
        }
    }

    #[test]
    fn preserves_composite_ids_precision_and_missing_source_time() {
        assert_eq!(id(&serde_json::json!("721-101902")).unwrap(), "721-101902");
        assert_eq!(id(&serde_json::json!("00023")).unwrap(), "00023");
        assert!(id(&serde_json::json!(1.5)).is_err());
        assert!(id(&serde_json::json!(-1)).is_err());
        assert!(id(&serde_json::from_str::<Value>("1e3").unwrap()).is_err());
        assert_eq!(id(&serde_json::json!(123)).unwrap(), "123");

        let value = super::super::wire::parse(b"0.10000000000000000001").unwrap();
        assert!(fixed::<Amount>(&value).is_err());

        assert_eq!(source_time(&Value::Null).unwrap(), None);
        assert_eq!(
            source_time(&serde_json::json!("2024-01-01 08:00:00")).unwrap().unwrap().unix_secs(),
            1704067200
        );
    }
}
