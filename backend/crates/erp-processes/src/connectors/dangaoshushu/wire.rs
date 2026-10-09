use std::fmt;

use erp_supply::ports::connector::common::ConnectorResult;
use serde::de::{MapAccess, Visitor};
use serde::{Deserialize, Deserializer};
use serde_json::value::RawValue;
use serde_json::{Map, Number, Value, from_slice, from_str};

use super::parsing::mapping;

const MAX_DEPTH: usize = 32;
const MAX_DECIMAL_LEN: usize = 4096;

/// 按供应商原始数字词法解析响应，避免金额或标识先经过浮点数。
///
/// # 参数
/// - `body`：完整 JSON 响应字节；传输层负责响应大小限制。
///
/// # 返回
/// 返回保留精确数字的 JSON；小数及指数数字转成十进制字符串。
///
/// # 错误
/// JSON 非法、重复字段、嵌套过深、数字身份不合法或十进制展开过长时失败。
pub(super) fn parse(body: &[u8]) -> ConnectorResult<Value> {
    let raw: Box<RawValue> = from_slice(body).map_err(|_| mapping())?;
    value(&raw, None, 0)
}

fn value(raw: &RawValue, field: Option<&str>, depth: usize) -> ConnectorResult<Value> {
    if depth > MAX_DEPTH {
        return Err(mapping());
    }
    let token = raw.get().trim();
    match token.as_bytes().first() {
        Some(b'{') => object(token, depth),
        Some(b'[') => array(token, field, depth),
        Some(b'"') => from_str(token).map(Value::String).map_err(|_| mapping()),
        _ if token == "null" => Ok(Value::Null),
        _ if token == "true" => Ok(Value::Bool(true)),
        _ if token == "false" => Ok(Value::Bool(false)),
        _ => number(token, field),
    }
}

fn object(token: &str, depth: usize) -> ConnectorResult<Value> {
    let raw: RawObject = from_str(token).map_err(|_| mapping())?;
    let mut result = Map::new();
    for (key, raw) in raw.0 {
        if result.contains_key(&key) {
            return Err(mapping());
        }
        let parsed = value(&raw, Some(&key), depth + 1)?;
        result.insert(key, parsed);
    }
    Ok(Value::Object(result))
}

fn array(token: &str, field: Option<&str>, depth: usize) -> ConnectorResult<Value> {
    let raw: Vec<Box<RawValue>> = from_str(token).map_err(|_| mapping())?;
    raw.into_iter()
        .map(|raw| value(&raw, field, depth + 1))
        .collect::<ConnectorResult<Vec<_>>>()
        .map(Value::Array)
}

fn number(token: &str, field: Option<&str>) -> ConnectorResult<Value> {
    if field.is_some_and(identity) && !token.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(mapping());
    }
    if token.contains(['.', 'e', 'E']) {
        return decimal(token).map(Value::String);
    }
    if let Ok(integer) = token.parse::<u64>() {
        return Ok(Value::Number(Number::from(integer)));
    }
    if let Ok(integer) = token.parse::<i64>() {
        return Ok(Value::Number(Number::from(integer)));
    }
    Ok(Value::String(token.to_owned()))
}

fn identity(field: &str) -> bool {
    matches!(
        field,
        "id" | "spec_id"
            | "product_id"
            | "city_id"
            | "brand_id"
            | "rule_id"
            | "distribution_rule_id"
            | "shop_id"
            | "user_id"
            | "order_no"
            | "out_order_no"
    )
}

fn decimal(token: &str) -> ConnectorResult<String> {
    if token.len() > MAX_DECIMAL_LEN {
        return Err(mapping());
    }
    let Some((mantissa, exponent)) = token.split_once(['e', 'E']) else {
        return Ok(token.to_owned());
    };
    let exponent: i32 = exponent.parse().map_err(|_| mapping())?;
    let negative = mantissa.starts_with('-');
    let unsigned = mantissa.strip_prefix('-').unwrap_or(mantissa);
    let integer_len = unsigned.find('.').unwrap_or(unsigned.len());
    let position =
        i32::try_from(integer_len).map_err(|_| mapping())?.checked_add(exponent).ok_or_else(mapping)?;
    let digits = unsigned.replace('.', "");
    let expanded = expand(&digits, position)?;
    Ok(if negative { format!("-{expanded}") } else { expanded })
}

fn expand(digits: &str, position: i32) -> ConnectorResult<String> {
    if position <= 0 {
        let padding =
            position.checked_neg().and_then(|count| usize::try_from(count).ok()).ok_or_else(mapping)?;
        let length =
            digits.len().checked_add(padding).and_then(|count| count.checked_add(2)).ok_or_else(mapping)?;
        if length > MAX_DECIMAL_LEN {
            return Err(mapping());
        }
        return Ok(format!("0.{}{digits}", "0".repeat(padding)));
    }
    let position = usize::try_from(position).map_err(|_| mapping())?;
    if position >= digits.len() {
        if position > MAX_DECIMAL_LEN {
            return Err(mapping());
        }
        return Ok(format!("{digits}{}", "0".repeat(position - digits.len())));
    }
    let (integer, fraction) = digits.split_at(position);
    Ok(format!("{integer}.{fraction}"))
}

struct RawObject(Vec<(String, Box<RawValue>)>);

impl<'de> Deserialize<'de> for RawObject {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_map(ObjectVisitor)
    }
}

struct ObjectVisitor;

impl<'de> Visitor<'de> for ObjectVisitor {
    type Value = RawObject;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a JSON object")
    }

    fn visit_map<A: MapAccess<'de>>(self, mut entries: A) -> Result<Self::Value, A::Error> {
        let mut fields = Vec::new();
        while let Some(field) = entries.next_entry()? {
            fields.push(field);
        }
        Ok(RawObject(fields))
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::io;

    use erp_core::money::Amount;
    use serde::Serialize;
    use serde_json::json;
    use serde_json::ser::{CompactFormatter, Formatter, Serializer};

    use super::*;
    use crate::connectors::dangaoshushu::parsing::fixed;

    #[test]
    fn preserves_decimal_digits_and_rejects_unsupported_amount_precision() {
        let value =
            parse(br#"{"clearing_price":0.10000000000000000001,"fee":1.00000000000000000001e-2}"#).unwrap();
        assert_eq!(value["clearing_price"], "0.10000000000000000001");
        assert_eq!(value["fee"], "0.0100000000000000000001");
        assert!(fixed::<Amount>(&value["clearing_price"]).is_err());
        assert!(fixed::<Amount>(&value["fee"]).is_err());
        assert_eq!(parse(br#"{"fee":-1.25e2,"other":2e3}"#).unwrap(), json!({"fee":"-125", "other":"2000"}));
    }

    #[test]
    fn preserves_integer_and_composite_identities_and_json_semantics() {
        let value = parse(br#"{"id":123,"brand_id":"721-101902","spec_id":"00023","huge":18446744073709551616,"count":-9999999,"enabled":true,"missing":null,"rows":[{"city_id":110100}]}"#).unwrap();
        assert_eq!(value["id"], json!(123));
        assert_eq!(value["brand_id"], "721-101902");
        assert_eq!(value["spec_id"], "00023");
        assert_eq!(value["huge"], "18446744073709551616");
        assert_eq!(value["count"], json!(-9999999));
        assert_eq!(value["enabled"], json!(true));
        assert!(value["missing"].is_null());
        assert_eq!(value["rows"][0]["city_id"], json!(110100));
    }

    #[test]
    fn rejects_fractional_exponent_or_negative_numeric_identities() {
        for field in [
            "id",
            "spec_id",
            "product_id",
            "city_id",
            "brand_id",
            "rule_id",
            "distribution_rule_id",
            "shop_id",
            "user_id",
            "order_no",
            "out_order_no",
        ] {
            for token in ["1.0", "1e3", "1E0", "-1", "-0"] {
                let body = format!(r#"{{"{field}":{token}}}"#);
                assert!(parse(body.as_bytes()).is_err(), "{field}: {token}");
            }
        }
    }

    #[test]
    fn rejects_duplicate_keys_invalid_json_depth_and_unbounded_expansion() {
        for body in [
            br#"{"id":1,"id":2}"#.as_slice(),
            br#"{"name":"a","\u006eame":"b"}"#,
            b"01",
            b"NaN",
            b"1e999999999999",
            b"1e4097",
            b"1e-4097",
        ] {
            assert!(parse(body).is_err());
        }
        let accepted = format!("{}0{}", "[".repeat(MAX_DEPTH), "]".repeat(MAX_DEPTH));
        assert!(parse(accepted.as_bytes()).is_ok());
        let too_deep = format!("[{accepted}]");
        assert!(parse(too_deep.as_bytes()).is_err());
    }

    #[test]
    fn ordinary_json_integer_keeps_numeric_serialization_separate_from_exact_vendor_decimal() {
        let writes = Cell::new(0);
        let mut output = Vec::new();
        let mut serializer = Serializer::with_formatter(&mut output, IntegerFormatter(&writes));
        json!({"id": 123}).serialize(&mut serializer).unwrap();
        assert_eq!(writes.get(), 1);
        assert_eq!(output, br#"{"id":123}"#);

        let exact = parse(br#"{"id":123,"price":0.10000000000000000001}"#).unwrap();
        assert_eq!(serde_json::to_string(&exact).unwrap(), r#"{"id":123,"price":"0.10000000000000000001"}"#);
    }

    struct IntegerFormatter<'a>(&'a Cell<usize>);

    impl Formatter for IntegerFormatter<'_> {
        fn write_u64<W: io::Write + ?Sized>(&mut self, writer: &mut W, value: u64) -> io::Result<()> {
            self.0.set(self.0.get() + 1);
            CompactFormatter.write_u64(writer, value)
        }

        fn write_i64<W: io::Write + ?Sized>(&mut self, writer: &mut W, value: i64) -> io::Result<()> {
            self.0.set(self.0.get() + 1);
            CompactFormatter.write_i64(writer, value)
        }
    }
}
