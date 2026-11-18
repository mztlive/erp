//! 保留完整供应商错误正文，并展开常见错误字段供查询。
use serde_json::Value;

#[derive(Default)]
pub(super) struct Rejection {
    pub(super) body_state: Option<&'static str>,
    pub(super) body: Option<String>,
    pub(super) read_error: Option<String>,
    pub(super) kind: Option<String>,
    pub(super) code: Option<String>,
    pub(super) param: Option<String>,
    pub(super) message: Option<String>,
}

impl Rejection {
    // 收到非成功 HTTP 状态但还未读完正文时标记 pending，区分上游已拒绝与尚未收到任何响应。
    pub(super) fn pending() -> Self {
        Self { body_state: Some("pending"), ..Self::default() }
    }

    // 读取拒绝正文出错时标记 read_failed 并保存诊断文本，不将部分内容当作完整正文。
    pub(super) fn read_failed(error: String) -> Self {
        Self { body_state: Some("read_failed"), read_error: Some(error), ..Self::default() }
    }

    // 拒绝正文读完或从 SDK 取得时标记 received，保留完整文本；能解析 JSON 时再展开 type、code、param 和 message 供查询。
    pub(super) fn parse(body: String) -> Self {
        let value = serde_json::from_str::<Value>(&body);
        let mut result = Self { body_state: Some("received"), body: Some(body), ..Self::default() };
        if let Ok(value) = value {
            let error = value.get("error").unwrap_or(&value);
            result.kind = field(error, "type");
            result.code = field(error, "code");
            result.param = field(error, "param");
            result.message = field(error, "message");
        }
        result
    }
}

fn field(value: &Value, name: &str) -> Option<String> {
    let value = value.get(name).filter(|value| !value.is_null())?;
    Some(value.as_str().map(str::to_owned).unwrap_or_else(|| value.to_string()))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn preserves_unknown_fields_values_and_complete_long_body() {
        let message = format!("不支持 json_schema\n{}", "详细错误".repeat(10_000));
        let body = json!({"error": {"type": "vendor.custom", "code": 400,
            "param": "messages[0].content", "message": message, "details": {"reason": "原始诊断"}}})
        .to_string();
        let result = Rejection::parse(body.clone());
        assert_eq!(result.body.as_deref(), Some(body.as_str()));
        assert_eq!(result.kind.as_deref(), Some("vendor.custom"));
        assert_eq!(result.code.as_deref(), Some("400"));
        assert_eq!(result.param.as_deref(), Some("messages[0].content"));
        assert_eq!(result.message.as_deref(), Some(message.as_str()));
    }

    #[test]
    fn present_error_key_never_falls_back_to_outer_message() {
        for error in [Value::Null, json!({}), json!([]), json!("denied"), json!(false), json!(400)] {
            let body = json!({"error": error, "message": "outer message", "code": "outer-code"}).to_string();
            let result = Rejection::parse(body.clone());
            assert_eq!(result.body_state, Some("received"));
            assert_eq!(result.body.as_deref(), Some(body.as_str()));
            assert_eq!(result.kind, None);
            assert_eq!(result.code, None);
            assert_eq!(result.param, None);
            assert_eq!(result.message, None);
        }
    }

    #[test]
    fn preserves_non_json_and_flat_errors_without_changing_them() {
        for body in ["", "<html>完整网关错误</html>", "{\"error\":", "null"] {
            let result = Rejection::parse(body.into());
            assert_eq!(result.body.as_deref(), Some(body));
            assert_eq!(result.body_state, Some("received"));
            assert_eq!(result.code, None);
        }

        let result = Rejection::parse(r#"{"code":"custom-code","message":"原始消息","param":null}"#.into());
        assert_eq!(result.code.as_deref(), Some("custom-code"));
        assert_eq!(result.message.as_deref(), Some("原始消息"));
        assert_eq!(result.param, None);
    }
}
