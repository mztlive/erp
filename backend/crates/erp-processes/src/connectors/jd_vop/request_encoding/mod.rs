//! 首期业务 API 的官方请求封装。字段名依据各方法文档，不从任意 JSON 推导账号身份。
use std::collections::BTreeMap;

use erp_supply::ports::connector::common::ConnectorResult;
use serde::ser::SerializeMap;
use serde::{Serialize, Serializer as SerdeSerializer};
use serde_json::value::{RawValue, to_raw_value};
use serde_json::{Value, json};

use super::mapping;
use super::order_payload::identifier;
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
    /// 固化业务调用渠道，不要求调用方提供网关系统字段。
    /// # 参数
    /// `source_type` 来自部署配置；配置默认3，允许明确覆盖为合法 Java int。
    /// # 返回
    /// 仅包含渠道的上下文。
    /// # 错误
    /// 渠道不是正 Java int 时返回脱敏映射错误。
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

/// 编码已白名单化的只读请求，返回不会丢失长整数的原始业务 JSON。
/// # 参数
/// `request` 为标准读取意图，`context` 为受信账号上下文。
/// # 返回
/// 含官方方法 wrapper 的业务 JSON，不包含访问令牌和签名。
/// # 错误
/// 数量、标识、渠道或方法字段不合法时返回脱敏错误。
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

/// 编码唯一外部写动作，不改变原动作、价格、目标或业务范围。
/// # 参数
/// `request` 为持久化动作的类型化请求，`context` 为受信账号上下文。
/// # 返回
/// 官方请求 JSON；创建金额直接序列化，不经浮点数中转。
/// # 错误
/// 渠道、目标、批次数或载荷不合法时返回脱敏错误。
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
    let Value::Object(fields) = fields else {
        return Err(mapping("京东业务请求结构无效"));
    };
    // 不让依赖是否启用 serde_json/preserve_order 改变业务参数的固定字节顺序。
    let mut object: BTreeMap<String, Value> = fields.into_iter().collect();
    object.insert("sourceType".into(), json!(context.source_type));
    envelope(name, &object, None)
}

fn number(value: &str, zero: bool) -> ConnectorResult<u64> {
    if zero && value == "0" {
        return Ok(0);
    }
    identifier(value)
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
        json!({"provinceId":identifier(&values[0])?,"cityId":identifier(&values[1])?,"countyId":identifier(&values[2])?,"townId":number(&values[3],true)?}),
    )
}
