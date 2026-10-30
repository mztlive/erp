//! 京东 VOP 官方 HTTP 签名和参数编码；本模块不发请求、不读取令牌，也不构成账号验收。
//!
//! 算法依据：https://vop.jd.com/doc/guide?id=93e984c4-2fd3-4b5b-9745-1b197d30d6ab 。
//! 排除 sign 后按参数名排序，拼接原始参数值，首尾加入 appSecret，以 UTF-8 计算大写 MD5。
//! 业务 JSON 只序列化一次，签名后对同一份参数进行 URL 编码；不沿用官方页面不一致的示例摘要。
use std::collections::BTreeMap;

use erp_supply::ports::connector::common::ConnectorResult;
use md5::Context;
use serde::Serialize;
use serde_json::to_string;
use serde_json::value::RawValue;
use url::form_urlencoded::Serializer;

use super::mapping;

const BUSINESS_PARAMETER: &str = "360buy_param_json";
const REQUIRED_PARAMETERS: [&str; 6] = ["method", "access_token", "app_key", "timestamp", "format", "v"];

/// 已签名的 URL 编码参数，含令牌且不提供 Debug，调用方须按原串使用，不能再次编码。
pub struct SignedParameters {
    encoded: String,
}

impl SignedParameters {
    /// 借用签名后冻结的参数串。
    /// # 参数
    /// 无。
    /// # 返回
    /// 可直接用作 URL 查询部分或表单正文的编码串；不得写入日志。
    /// # 错误
    /// 不返回错误，不执行 I/O。
    pub fn as_str(&self) -> &str {
        &self.encoded
    }

    /// 取得签名后冻结的参数串所有权。
    /// # 参数
    /// 无。
    /// # 返回
    /// 与签名时同一份原始参数对应的编码串，包含签名及访问令牌。
    /// # 错误
    /// 不返回错误，不执行 I/O。
    pub fn into_string(self) -> String {
        self.encoded
    }
}

/// 按京东 VOP 官方规则签名并编码，公共参数和业务 JSON 仅形成一份请求内容。
/// # 参数
/// `parameters` 提供实际公共及扩展参数，不补造版本、时间或令牌；旧 sign 自动排除。
/// `business` 为类型化业务载荷，`app_secret` 单独传入且不作为请求参数发送。
/// # 返回
/// 业务 JSON 一次序列化、参数原值签名及 URL 编码后的冻结参数串。
/// # 错误
/// 公共参数缺失、格式不支持、密钥被混入参数、业务参数重复或序列化失败时返回脱敏错误。
pub fn encode<B: Serialize + ?Sized>(
    mut parameters: BTreeMap<String, String>,
    business: &B,
    app_secret: &str,
) -> ConnectorResult<SignedParameters> {
    parameters.remove("sign");
    validate(&parameters, app_secret)?;
    let business = to_string(business).map_err(|_| mapping("京东业务参数无法编码，请核对请求资料"))?;
    finish(parameters, business, app_secret)
}

/// 对已完成类型化序列化的业务 JSON 签名，保持数字和 JSON 原文不变。
/// # 参数
/// 公共参数、原始业务 JSON 和独立的应用密钥。
/// # 返回
/// 直接复用业务 JSON 字节的冻结参数串。
/// # 错误
/// 公共参数或密钥边界不符合协议时返回脱敏错误。
pub(super) fn encode_raw(
    mut parameters: BTreeMap<String, String>,
    business: &RawValue,
    app_secret: &str,
) -> ConnectorResult<SignedParameters> {
    parameters.remove("sign");
    validate(&parameters, app_secret)?;
    finish(parameters, business.get().to_owned(), app_secret)
}

fn finish(
    mut parameters: BTreeMap<String, String>,
    business: String,
    app_secret: &str,
) -> ConnectorResult<SignedParameters> {
    parameters.insert(BUSINESS_PARAMETER.into(), business);
    let signature = sign(&parameters, app_secret);
    parameters.insert("sign".into(), signature);
    let encoded = Serializer::new(String::new()).extend_pairs(&parameters).finish();
    Ok(SignedParameters { encoded })
}

fn validate(parameters: &BTreeMap<String, String>, app_secret: &str) -> ConnectorResult<()> {
    if app_secret.is_empty()
        || REQUIRED_PARAMETERS
            .iter()
            .any(|key| parameters.get(*key).is_none_or(|value| value.trim().is_empty()))
        || parameters.get("format").map(String::as_str) != Some("json")
    {
        return Err(mapping("京东签名公共参数或应用密钥不完整，请核对接入资料"));
    }
    if parameters.keys().any(|key| {
        key.eq_ignore_ascii_case("app_secret")
            || key.eq_ignore_ascii_case("appSecret")
            || key == BUSINESS_PARAMETER
    }) {
        return Err(mapping("京东应用密钥须单独提供，业务参数须通过类型化载荷编码"));
    }
    Ok(())
}

fn sign(parameters: &BTreeMap<String, String>, app_secret: &str) -> String {
    let mut context = Context::new();
    context.consume(app_secret.as_bytes());
    for (key, value) in parameters {
        context.consume(key.as_bytes());
        context.consume(value.as_bytes());
    }
    context.consume(app_secret.as_bytes());
    format!("{:X}", context.finalize())
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use serde::Serializer as SerdeSerializer;
    use serde::ser::{Error, SerializeMap};
    use serde_json::json;
    use url::form_urlencoded::parse;

    use super::*;

    const SECRET: &str = "offline-secret";
    // 独立离线向量：固定参数的手工升序拼接串由 Python hashlib / urllib.parse 计算。
    // 不是官方账号凭据、官方页面示例摘要或本生产方法计算出的测试期望。
    const BASIC_SIGNATURE: &str = "54778568A0B7A33E6603ACEAA87D337D";
    const BASIC_ENCODED: &str = "360buy_param_json=%7B%7D&access_token=token-1&app_key=app-1&format=json&method=jingdong.vop.goods.getSkuPoolInfo&sign=54778568A0B7A33E6603ACEAA87D337D&timestamp=2026-10-30+12%3A34%3A56&v=2.0";
    const UNICODE_ENCODED: &str = "360buy_param_json=%7B%22text%22%3A%22%E4%B8%AD%E6%96%87+%E7%A9%BA%E6%A0%BC%26%E9%94%AE%3D%E5%80%BC%25100+%2B+%3F%5C%22%5C%5C%2F%5Cn%22%7D&access_token=token-1&app_key=app-1&format=json&memo=%E5%A4%87%E6%B3%A8+%E7%A9%BA%E6%A0%BC+%26+%3D+%25+%2B+%2F&method=jingdong.vop.goods.getSkuPoolInfo&sign=DB50AF173751AE4D26909B7C2CCE6767&timestamp=2026-10-30+12%3A34%3A56&v=2.0";

    fn parameters() -> BTreeMap<String, String> {
        [
            ("v", "2.0"),
            ("timestamp", "2026-10-30 12:34:56"),
            ("format", "json"),
            ("method", "jingdong.vop.goods.getSkuPoolInfo"),
            ("app_key", "app-1"),
            ("access_token", "token-1"),
        ]
        .into_iter()
        .map(|(key, value)| (key.into(), value.into()))
        .collect()
    }

    fn decoded(value: &SignedParameters) -> BTreeMap<String, String> {
        parse(value.as_str().as_bytes()).into_owned().collect()
    }

    #[test]
    fn fixed_vector_sorts_original_values_and_replaces_previous_signature() {
        let expected = encode(parameters(), &json!({}), SECRET).unwrap();
        assert_eq!(expected.as_str(), BASIC_ENCODED);
        assert_eq!(decoded(&expected)["sign"], BASIC_SIGNATURE);
        let mut reversed = parameters().into_iter().rev().collect::<BTreeMap<_, _>>();
        reversed.insert("sign".into(), "old%26signature&must=not-participate".into());
        assert_eq!(encode(reversed, &json!({}), SECRET).unwrap().into_string(), BASIC_ENCODED);
        assert!(!expected.as_str().contains(SECRET));
        assert!(!decoded(&expected).contains_key("app_secret"));
    }

    struct CountedPayload<'a> {
        count: &'a AtomicUsize,
        text: &'a str,
    }

    impl Serialize for CountedPayload<'_> {
        fn serialize<S: SerdeSerializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
            self.count.fetch_add(1, Ordering::SeqCst);
            let mut map = serializer.serialize_map(Some(1))?;
            map.serialize_entry("text", self.text)?;
            map.end()
        }
    }

    #[test]
    fn unicode_json_and_reserved_characters_are_serialized_once_then_url_encoded() {
        let count = AtomicUsize::new(0);
        let mut parameters = parameters();
        parameters.insert("memo".into(), "备注 空格 & = % + /".into());
        let payload = CountedPayload { count: &count, text: "中文 空格&键=值%100 + ?\"\\/\n" };
        let signed = encode(parameters, &payload, SECRET).unwrap();
        assert_eq!(count.load(Ordering::SeqCst), 1);
        assert_eq!(signed.as_str(), UNICODE_ENCODED);
        let values = decoded(&signed);
        assert_eq!(values["sign"], "DB50AF173751AE4D26909B7C2CCE6767");
        assert_eq!(values[BUSINESS_PARAMETER], r#"{"text":"中文 空格&键=值%100 + ?\"\\/\n"}"#);
        assert_eq!(values["memo"], "备注 空格 & = % + /");
    }

    #[test]
    fn actual_empty_extension_values_participate_and_payload_changes_reach_the_signed_request() {
        let mut extended = parameters();
        extended.insert("empty".into(), String::new());
        let signed = encode(extended.clone(), &json!({"skuId":"123"}), SECRET).unwrap();
        let values = decoded(&signed);
        assert_eq!(values["empty"], "");
        assert_eq!(values["sign"], "081B484C03B40D50DE2B00A0614F587F");
        assert_eq!(values[BUSINESS_PARAMETER], r#"{"skuId":"123"}"#);
        let changed = encode(extended, &json!({"skuId":"456"}), SECRET).unwrap();
        assert_eq!(decoded(&changed)[BUSINESS_PARAMETER], r#"{"skuId":"456"}"#);
        assert_ne!(values["sign"], decoded(&changed)["sign"]);
    }

    #[test]
    fn invalid_public_parameters_and_secret_injection_are_rejected_without_exposing_values() {
        for key in REQUIRED_PARAMETERS {
            let mut missing = parameters();
            missing.remove(key);
            assert!(encode(missing, &json!({}), SECRET).is_err());
            let mut blank = parameters();
            blank.insert(key.into(), " ".into());
            assert!(encode(blank, &json!({}), SECRET).is_err());
        }
        for key in ["app_secret", "appSecret", "APP_SECRET", BUSINESS_PARAMETER] {
            let mut injected = parameters();
            injected.insert(key.into(), "never-include-this-private-value".into());
            let error = encode(injected, &json!({}), SECRET).err().unwrap();
            assert!(!error.summary.contains("never-include"));
            assert!(!error.summary.contains(SECRET));
        }
        assert!(encode(parameters(), &json!({}), "").is_err());
        let mut xml = parameters();
        xml.insert("format".into(), "xml".into());
        assert!(encode(xml, &json!({}), SECRET).is_err());
    }

    struct InvalidPayload;

    impl Serialize for InvalidPayload {
        fn serialize<S: SerdeSerializer>(&self, _: S) -> Result<S::Ok, S::Error> {
            Err(S::Error::custom("never-expose-payload-or-token"))
        }
    }

    #[test]
    fn serialization_errors_are_sanitized() {
        let error = encode(parameters(), &InvalidPayload, SECRET).err().unwrap();
        assert_eq!(error.code, "JD_VOP_MAPPING_INVALID");
        assert!(!error.summary.contains("never-expose"));
        assert!(!error.summary.contains(SECRET));
    }
}
