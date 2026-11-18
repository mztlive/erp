//! 按各方法的网关协议编码顶层字段或请求对象，不从任意 JSON 推导账号身份。
use std::collections::BTreeMap;

use erp_supply::ports::connector::common::ConnectorResult;
use serde::ser::SerializeMap;
use serde::{Serialize, Serializer as SerdeSerializer};
use serde_json::value::{RawValue, to_raw_value};
use serde_json::{Value, json};

use super::mapping;
use super::order_payload::{identifier, identifier_or_zero};
use super::transport::{JdVopReadRequest, JdVopWriteRequest};

mod catalog;
mod orders;
#[cfg(test)]
mod tests;

/// 由部署配置确定的业务调用渠道；系统字段由京东网关按应用授权提供。
pub struct JdVopRequestContext {
    source_type: u32,
}

impl JdVopRequestContext {
    /// 装配请求编码器时固定部署的 `source_type`（配置默认 3），供各方法复用同一业务渠道；网关系统字段无需由调用方传入。
    /// 渠道必须能表示为正 Java `int`，否则返回脱敏映射错误。
    pub fn new(source_type: u32) -> ConnectorResult<Self> {
        if source_type == 0 || source_type > u32::try_from(i32::MAX).expect("int正上限可表示") {
            return Err(mapping("京东调用渠道超出接口范围"));
        }
        Ok(Self { source_type })
    }

    fn source(&self, source: u32) -> ConnectorResult<()> {
        if source != self.source_type {
            return Err(mapping("京东请求渠道与受控账号不一致"));
        }
        Ok(())
    }
}

/// 签名前，把白名单查询按受信账号和对应网关方法的参数层级编码成保留长整数的业务 JSON，不包含访问令牌或签名。
/// 数量、标识、渠道或方法字段非法时返回脱敏错误。
pub fn read_payload(
    request: &JdVopReadRequest,
    context: &JdVopRequestContext,
) -> ConnectorResult<Box<RawValue>> {
    use JdVopReadRequest as R;
    match request {
        R::Pools
        | R::PoolPage { .. }
        | R::Detail { .. }
        | R::Price { .. }
        | R::Images { .. }
        | R::Categories { .. }
        | R::Areas { .. }
        | R::VerifyAddress { .. } => catalog::read(request, context),
        _ => orders::read(request, context),
    }
}

/// 签名前，把已持久化的单个写动作转成官方业务 JSON，不替换动作、目标、价格或批次范围。
/// 创建金额直接按十进制序列化，不经过浮点数；渠道、目标、批次或载荷不合法时返回脱敏错误。
pub fn write_payload(
    request: &JdVopWriteRequest,
    context: &JdVopRequestContext,
) -> ConnectorResult<Box<RawValue>> {
    orders::write(request, context)
}

struct Envelope<'a, T> {
    name: &'static str,
    value: &'a T,
    outer: Option<&'a JdVopRequestContext>,
}

impl<T: Serialize> Serialize for Envelope<'_, T> {
    fn serialize<S: SerdeSerializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry(self.name, self.value)?;
        if let Some(context) = self.outer {
            map.serialize_entry("sourceType", &context.source_type)?;
        }
        map.end()
    }
}

fn envelope<T: Serialize>(
    name: &'static str,
    value: &T,
    outer: Option<&JdVopRequestContext>,
) -> ConnectorResult<Box<RawValue>> {
    to_raw_value(&Envelope { name, value, outer }).map_err(|_| mapping("京东业务请求无法编码"))
}

fn nested(
    name: &'static str,
    fields: Value,
    context: &JdVopRequestContext,
) -> ConnectorResult<Box<RawValue>> {
    envelope(name, &fields_with_source(fields, context)?, None)
}

// 京东方法要求顶层业务参数时，用稳定字段顺序输出原始 JSON，保留长编号精度供签名和发送复用。
fn flat(fields: Value, context: &JdVopRequestContext) -> ConnectorResult<Box<RawValue>> {
    to_raw_value(&fields_with_source(fields, context)?).map_err(|_| mapping("京东业务请求无法编码"))
}

// 在方法指定的唯一参数层注入部署渠道，避免调用方另填 sourceType；网关系统字段不在这里生成。
fn fields_with_source(
    fields: Value,
    context: &JdVopRequestContext,
) -> ConnectorResult<BTreeMap<String, Value>> {
    let Value::Object(fields) = fields else {
        return Err(mapping("京东业务请求结构无效"));
    };
    // 不让依赖是否启用 serde_json/preserve_order 改变业务参数的固定字节顺序。
    let mut object: BTreeMap<String, Value> = fields.into_iter().collect();
    object.insert("sourceType".into(), json!(context.source_type));
    Ok(object)
}

fn merchant(value: &str) -> ConnectorResult<()> {
    if value.trim().is_empty() || value.chars().count() > 100 || value.chars().any(char::is_control) {
        return Err(mapping("京东原第三方订单号无效"));
    }
    Ok(())
}

fn ids(values: &[String], maximum: usize) -> ConnectorResult<Vec<u64>> {
    if values.is_empty() || values.len() > maximum {
        return Err(mapping("京东请求编号数量超过接口范围"));
    }
    values.iter().map(|value| identifier(value)).collect()
}

fn area(values: &[String; 4]) -> ConnectorResult<Value> {
    Ok(
        json!({"provinceId":identifier(&values[0])?,"cityId":identifier(&values[1])?,"countyId":identifier(&values[2])?,"townId":identifier_or_zero(&values[3])?}),
    )
}
