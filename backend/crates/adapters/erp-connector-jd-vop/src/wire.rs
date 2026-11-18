//! 京东官方方法封装与标量解析；金额和长编号不经过浮点数。
use std::collections::BTreeMap;

use erp_supply::entity::failure::SupplierFailureClass;
use erp_supply::ports::connector::common::{ConnectorError, ConnectorResult};
use serde::de::{DeserializeOwned, Error as DeError};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::value::RawValue;

use super::{error, mapping};

/// 保留来源十进制文本，JSON 数字也从原词法值读取。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WireText(pub String);

impl<'de> Deserialize<'de> for WireText {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = Box::<RawValue>::deserialize(deserializer)?;
        let text = raw.get();
        let value = if text.starts_with('"') {
            serde_json::from_str::<String>(text).map_err(D::Error::custom)?
        } else {
            text.to_owned()
        };
        if value.is_empty() || value.len() > 128 || !value.bytes().all(|byte| byte.is_ascii_graphic()) {
            return Err(D::Error::custom("invalid supplier scalar"));
        }
        Ok(Self(value))
    }
}

/// 京东 long 编号按字符串保存，拒绝小数、指数、负数和布尔值。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct WireId(pub String);

impl<'de> Deserialize<'de> for WireId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = WireText::deserialize(deserializer)?.0;
        if value.len() > 30 || !value.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err(D::Error::custom("invalid supplier identifier"));
        }
        Ok(Self(value))
    }
}

#[derive(Deserialize)]
struct RpcResult {
    #[serde(rename = "success")]
    success: WireText,
    #[serde(rename = "resultCode")]
    code: WireText,
    result: Option<Box<RawValue>>,
}

// 收到 HTTP 正文后，按请求的官方方法键及 RPC 封装键取出业务结果，分别处理网关失败和业务失败，不以 HTTP 成功代替它们。
pub(super) fn decode<T: DeserializeOwned>(body: &[u8], method: &str, rpc: &str) -> ConnectorResult<T> {
    if body.len() > 8 * 1024 * 1024 {
        return Err(mapping("京东返回的资料过多，无法读取，请缩小查询范围或联系管理员核对"));
    }
    let mut root: BTreeMap<String, Box<RawValue>> = serde_json::from_slice(body)
        .map_err(|_| mapping("京东返回的资料无法读取，请联系管理员核对京东连接"))?;
    if let Some(failure) = root.remove("error_response") {
        return Err(gateway_error(failure.get())?);
    }
    validate_gateway_code(&root)?;

    let key = method.replace('.', "_");
    let wrapped = root
        .remove(&format!("{key}_responce"))
        .or_else(|| root.remove(&format!("{key}_response")))
        .ok_or_else(|| mapping("京东未返回所查询的业务资料，请联系管理员核对京东连接"))?;
    let mut method_result: BTreeMap<String, Box<RawValue>> = serde_json::from_str(wrapped.get())
        .map_err(|_| mapping("京东返回的查询资料无法读取，请联系管理员核对京东连接"))?;
    validate_gateway_code(&method_result)?;

    let result = method_result
        .remove(rpc)
        .ok_or_else(|| mapping("京东未返回所需的业务结果，请联系管理员核对京东连接"))?;
    let envelope: RpcResult = serde_json::from_str(result.get())
        .map_err(|_| mapping("京东返回的业务结果无法读取，请联系管理员核对京东连接"))?;
    decode_result(envelope, method)
}

fn validate_gateway_code(fields: &BTreeMap<String, Box<RawValue>>) -> ConnectorResult<()> {
    if let Some(code) = fields.get("code") {
        let code: WireText = serde_json::from_str(code.get())
            .map_err(|_| mapping("京东未提供可识别的失败原因，请联系管理员核对京东连接"))?;
        if code.0 != "0" {
            return Err(classify_code(&code.0));
        }
    }
    Ok(())
}

// 先核对 RPC 业务成功标记及结果码，再反序列化业务数据。只有消息轮询可按已验行为把缺失的结果视为空队列。
fn decode_result<T: DeserializeOwned>(envelope: RpcResult, method: &str) -> ConnectorResult<T> {
    let success_code = envelope.code.0 == "0000"
        || (method == "jingdong.vop.message.queryTransByVopNormal" && envelope.code.0 == "success");
    if envelope.success.0 != "true" || !success_code {
        return Err(classify_code(&envelope.code.0));
    }

    let result = match envelope.result.as_deref() {
        Some(result) => result.get(),
        None if method == "jingdong.vop.message.queryTransByVopNormal" => "[]",
        None => return Err(mapping("京东未返回必要的业务结果，请联系管理员核对查询范围")),
    };
    serde_json::from_str(result)
        .map_err(|_| mapping("京东返回的业务资料不符合要求，请联系管理员核对京东资料"))
}

fn gateway_error(body: &str) -> ConnectorResult<ConnectorError> {
    #[derive(Deserialize)]
    struct GatewayError {
        code: WireText,
    }
    let response: GatewayError = serde_json::from_str(body)
        .map_err(|_| mapping("京东未提供可识别的失败原因，请联系管理员核对京东连接"))?;
    Ok(classify_code(&response.code.0))
}

fn classify_code(code: &str) -> ConnectorError {
    let class = match code {
        "2001" | "2002" | "2003" => SupplierFailureClass::AuthSignature,
        "2010" | "2011" => SupplierFailureClass::RateLimited,
        "1001" | "1002" | "1003" | "1004" | "1005" => SupplierFailureClass::BusinessRejected,
        _ => SupplierFailureClass::CapabilityGap,
    };
    let code = if code.len() <= 32 && code.bytes().all(|byte| byte.is_ascii_alphanumeric() || byte == b'_') {
        format!("JD_VOP_{code}")
    } else {
        "JD_VOP_UNKNOWN_BUSINESS_CODE".into()
    };
    let summary = match class {
        SupplierFailureClass::AuthSignature => "京东未授权当前账号处理此业务，请联系管理员核对京东授权",
        SupplierFailureClass::RateLimited => "京东当前处理的请求较多，请稍后再试",
        SupplierFailureClass::BusinessRejected => {
            "京东未接受本次业务操作，请核对商品、订单或配送资料后再处理"
        },
        _ => "京东未确认支持本次业务，请联系管理员核对账号已开通的功能",
    };
    error(class, &code, summary)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Pool {
    pub biz_pool_id: WireId,
    pub name: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Pools {
    pub sku_pool_list: Vec<Pool>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PoolPage {
    pub skus: Vec<WireId>,
    pub offset: WireId,
    pub remain_page: WireId,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkuDetail {
    pub sku_id: WireId,
    pub sku_name: String,
    pub spu_id: Option<WireId>,
    pub spu_name: Option<String>,
    pub sale_unit: Option<String>,
    pub seo_model: Option<String>,
    pub color: Option<String>,
    pub size: Option<String>,
    pub capacity: Option<String>,
    pub ware_info: Option<String>,
    pub category: Option<String>,
    pub brand_name: Option<String>,
    pub lowest_buy: Option<WireText>,
    /// 官方 `queryExtSet=13` 返回的商品扩展；缺少不补造购买倍数。
    pub sku_ext_map: Option<SkuExtensions>,
    pub tax_rate_percentage: Option<WireText>,
    pub sku_state: Option<WireText>,
    pub image_path: Option<String>,
    #[serde(default)]
    pub param_group_attr_list: Vec<ParameterGroup>,
}

/// 仅解释已经核实的商品扩展键；兼容官方 Map 和文档 key/value 样例。
#[derive(Debug, Default)]
pub struct SkuExtensions {
    pub quantity_multiple: Option<WireText>,
}

impl<'de> Deserialize<'de> for SkuExtensions {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let mut fields = BTreeMap::<String, Box<RawValue>>::deserialize(deserializer)?;
        let direct = fields
            .remove("sfbsgm")
            .map(|raw| serde_json::from_str(raw.get()))
            .transpose()
            .map_err(D::Error::custom)?;
        let key = fields
            .remove("key")
            .map(|raw| serde_json::from_str::<String>(raw.get()))
            .transpose()
            .map_err(D::Error::custom)?;
        let pair = if key.as_deref() == Some("sfbsgm") {
            let raw = fields.remove("value").ok_or_else(|| D::Error::custom("missing supplier multiple"))?;
            Some(serde_json::from_str(raw.get()).map_err(D::Error::custom)?)
        } else {
            None
        };
        if direct.is_some() && pair.is_some() && direct != pair {
            return Err(D::Error::custom("conflicting supplier multiples"));
        }
        Ok(Self { quantity_multiple: direct.or(pair) })
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParameterGroup {
    pub param_group_name: String,
    pub param_attribute_list: Vec<Parameter>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Parameter {
    pub param_attr_name: String,
    pub param_attr_val_list: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SellPrice {
    pub sku_id: WireId,
    pub sale_price: Option<WireText>,
    pub tax_rate_percentage: Option<WireText>,
    pub has_promotion: Option<WireText>,
    pub promotion_type: Option<WireText>,
    pub fixed_price_promotion: Option<FixedPricePromotion>,
}

/// 一口价促销依赖客户资格及限购核验，本期只保留类型化来源信息。
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FixedPricePromotion {
    pub original_price: Option<WireText>,
    pub limited_num: Option<WireText>,
    pub remain_num: Option<WireText>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkuImages {
    pub sku_id: WireId,
    pub sku_image_list: Vec<SkuImage>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkuImage {
    pub short_path: String,
    pub is_primary: WireText,
    pub yn: WireText,
    pub order_sort: WireId,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Category {
    pub category_id: WireId,
    pub category_name: String,
    pub parent_id: WireId,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Areas {
    pub area_info_list: Vec<Area>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Area {
    pub area_id: WireId,
    pub area_name: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_successful_message_queries_allow_missing_or_null_results() {
        let method = "jingdong.vop.message.queryTransByVopNormal";
        for result in ["", ",\"result\":null", ",\"result\":[]"] {
            for code in ["0000", "success"] {
                let body = format!(
                    r#"{{"jingdong_vop_message_queryTransByVopNormal_responce":{{"openRpcResult":{{"success":true,"resultCode":"{code}"{result}}}}}}}"#
                );
                let records: Vec<Box<RawValue>> = decode(body.as_bytes(), method, "openRpcResult").unwrap();
                assert!(records.is_empty());
            }
        }

        let body = br#"{"jingdong_vop_message_queryTransByVopNormal_responce":{"openRpcResult":{"success":false,"resultCode":"1003"}}}"#;
        assert_eq!(
            decode::<Vec<Box<RawValue>>>(body, method, "openRpcResult").unwrap_err().code,
            "JD_VOP_1003"
        );

        let body = br#"{"jingdong_vop_message_queryTransByVopNormal_responce":{"openRpcResult":{"success":true,"resultCode":"0000","result":{}}}}"#;
        assert!(decode::<Vec<Box<RawValue>>>(body, method, "openRpcResult").is_err());

        let body = br#"{"jingdong_vop_goods_getSkuPoolInfo_responce":{"openRpcResult":{"success":true,"resultCode":"0000"}}}"#;
        assert!(
            decode::<Vec<Box<RawValue>>>(body, "jingdong.vop.goods.getSkuPoolInfo", "openRpcResult").is_err()
        );
    }

    #[test]
    fn gateway_codes_at_both_levels_preserve_success_failures_and_precedence() {
        use serde_json::{Value, json, to_vec};

        let method = "jingdong.vop.goods.getSkuPoolInfo";
        let key = "jingdong_vop_goods_getSkuPoolInfo_responce";
        let body = json!({key:{"openRpcResult":{"success":true,"resultCode":"0000",
            "result":{"skuPoolList":[]}}}});
        for code in [None, Some(json!(0)), Some(json!("0"))] {
            let mut success = body.clone();
            if let Some(code) = code {
                success["code"] = code.clone();
                success[key]["code"] = code;
            }
            let pools: Pools = decode(&to_vec(&success).unwrap(), method, "openRpcResult").unwrap();
            assert!(pools.sku_pool_list.is_empty());
        }

        for (code, expected) in [
            (json!(2002), classify_code("2002")),
            (json!("2010"), classify_code("2010")),
            (json!("FUTURE_99"), classify_code("FUTURE_99")),
            (Value::Null, classify_code("null")),
            (json!(" "), mapping("京东未提供可识别的失败原因，请联系管理员核对京东连接")),
        ] {
            let outer = json!({"code":code});
            assert_eq!(
                decode::<Pools>(&to_vec(&outer).unwrap(), method, "openRpcResult").unwrap_err(),
                expected
            );
            let inner = json!({"code":0,key:{"code":code}});
            assert_eq!(
                decode::<Pools>(&to_vec(&inner).unwrap(), method, "openRpcResult").unwrap_err(),
                expected
            );
        }

        for (body, expected) in [
            (json!({"error_response":{"code":"2002"},"code":"2010"}), classify_code("2002")),
            (json!({"code":"2010",key:{"code":"2002"}}), classify_code("2010")),
        ] {
            assert_eq!(
                decode::<Pools>(&to_vec(&body).unwrap(), method, "openRpcResult").unwrap_err(),
                expected
            );
        }
    }

    #[test]
    fn raw_decimal_and_long_identifiers_never_round_through_float() {
        assert_eq!(
            serde_json::from_str::<WireId>("90071992547409931234567890").unwrap().0,
            "90071992547409931234567890"
        );
        assert_eq!(serde_json::from_str::<WireText>("123.4567").unwrap().0, "123.4567");
        assert_eq!(serde_json::from_str::<WireId>(r#""001234""#).unwrap().0, "001234");
        for value in ["1.0", "1e3", "-1", "true", "null"] {
            assert!(serde_json::from_str::<WireId>(value).is_err());
        }
    }

    #[test]
    fn method_envelope_and_misspelling_are_explicit() {
        let body = br#"{"jingdong_vop_goods_getSkuPoolInfo_responce":{"openRpcResult":{"success":"true","resultCode":"0000","result":{"skuPoolList":[{"bizPoolId":3,"name":"pool"}]}}}}"#;
        let pools: Pools = decode(body, "jingdong.vop.goods.getSkuPoolInfo", "openRpcResult").unwrap();
        assert_eq!(pools.sku_pool_list[0].biz_pool_id.0, "3");
        assert!(decode::<Pools>(body, "jingdong.vop.goods.getSkuPoolInfo", "vopOrderRpcResult").is_err());
        assert!(decode::<Pools>(body, "jingdong.vop.goods.getSellPrice", "openRpcResult").is_err());
    }

    #[test]
    fn unknown_codes_fail_closed_and_upstream_messages_are_redacted() {
        let body = br#"{"error_response":{"code":"2002","en_desc":"secret account token"}}"#;
        let error = decode::<Pools>(body, "jingdong.vop.goods.getSkuPoolInfo", "openRpcResult").unwrap_err();
        assert_eq!(error.class, SupplierFailureClass::AuthSignature);
        assert!(!error.summary.contains("secret"));

        let body = br#"{"jingdong_vop_goods_getSkuPoolInfo_responce":{"openRpcResult":{"success":false,"resultCode":"FUTURE_99","resultMessage":"secret"}}}"#;
        let error = decode::<Pools>(body, "jingdong.vop.goods.getSkuPoolInfo", "openRpcResult").unwrap_err();
        assert_eq!(error.code, "JD_VOP_FUTURE_99");
        assert_eq!(error.class, SupplierFailureClass::CapabilityGap);
    }
}
