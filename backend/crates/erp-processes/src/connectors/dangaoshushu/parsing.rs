use std::str::FromStr;

use chrono::{FixedOffset, NaiveDateTime, TimeZone};
use erp_core::common::time::{BUSINESS_TZ_OFFSET_SECS, Instant};
use erp_supply::entity::failure::SupplierFailureClass;
use erp_supply::ports::connector::common::{
    ConnectorError, ConnectorResult, Snapshot, SourceRevision, SourceStamp,
};
use serde_json::Value;

use super::error;

pub(super) fn mapping() -> ConnectorError {
    error(SupplierFailureClass::MappingError, "DGSS_MAPPING", "供应商字段缺失或语义无法核实")
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
    if let Value::Number(number) = value
        && !number.to_string().bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err(mapping());
    }
    let value = text(value)?;
    valid_id(&value)?;
    Ok(value)
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

pub(super) fn source_time(value: &Value) -> ConnectorResult<Option<Instant>> {
    if value.is_null() || value.as_str() == Some("") {
        return Ok(None);
    }
    let text = text(value)?;
    let date = NaiveDateTime::parse_from_str(&text, "%Y-%m-%d %H:%M:%S").map_err(|_| mapping())?;
    let zone = FixedOffset::east_opt(BUSINESS_TZ_OFFSET_SECS).expect("业务时区固定为 +08:00");
    let date = zone.from_local_datetime(&date).single().ok_or_else(mapping)?;
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
    use erp_core::money::Amount;

    use super::*;
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
