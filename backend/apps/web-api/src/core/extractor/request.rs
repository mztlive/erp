//! 请求解码统一返回业务信封，不把框架的英文正文直接交给页面。

use std::collections::BTreeMap;
use std::error::Error as StdError;

use axum::extract::rejection::{JsonRejection, QueryRejection};
use axum::extract::{FromRequest, FromRequestParts, Request};
use axum::http::request::Parts;
use axum::http::{StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Error as JsonError;
use serde_path_to_error::Error as JsonPathError;

use crate::core::response::ApiResponse;

/// JSON 请求提取器，成功行为与 Axum Json 相同。
#[derive(Debug)]
pub struct Json<T>(pub T);

impl<S, T> FromRequest<S> for Json<T>
where
    S: Send + Sync,
    T: DeserializeOwned,
{
    type Rejection = RequestError;

    async fn from_request(request: Request, state: &S) -> Result<Self, Self::Rejection> {
        axum::Json::<T>::from_request(request, state)
            .await
            .map(|value| Self(value.0))
            .map_err(RequestError::json)
    }
}

impl<T: Serialize> IntoResponse for Json<T> {
    fn into_response(self) -> Response {
        axum::Json(self.0).into_response()
    }
}

/// 查询参数提取器，失败仍保留真实 HTTP 状态。
#[derive(Debug)]
pub struct Query<T>(pub T);

impl<S, T> FromRequestParts<S> for Query<T>
where
    S: Send + Sync,
    T: DeserializeOwned,
{
    type Rejection = RequestError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        axum::extract::Query::<T>::from_request_parts(parts, state)
            .await
            .map(|value| Self(value.0))
            .map_err(RequestError::query)
    }
}

impl<T: DeserializeOwned> Query<T> {
    /// 从 URI 解码查询条件。
    ///
    /// # 参数
    /// `uri` 为当前请求 URI。
    /// # 返回
    /// 强类型查询条件。
    /// # 错误
    /// 参数格式错误返回统一协议错误。
    pub fn try_from_uri(uri: &Uri) -> Result<Self, RequestError> {
        axum::extract::Query::<T>::try_from_uri(uri).map(|value| Self(value.0)).map_err(RequestError::query)
    }
}

/// 仅持有公开的协议错误说明，不保留可能含敏感输入的反序列化消息。
#[derive(Debug)]
pub struct RequestError {
    status: StatusCode,
    code: &'static str,
    message: &'static str,
    fields: Option<BTreeMap<String, String>>,
}

impl RequestError {
    /// 获取请求拒绝的 HTTP 状态。
    ///
    /// # 参数
    /// 无。
    /// # 返回
    /// 与响应信封一致的状态码。
    /// # 错误
    /// 无。
    pub fn status(&self) -> StatusCode {
        self.status
    }

    fn json(error: JsonRejection) -> Self {
        let status = error.status();
        let (code, message) = match &error {
            JsonRejection::JsonDataError(_) => (
                "REQUEST_FIELD_INVALID",
                "提交资料的字段缺失或格式不正确，请核对字段提示；如未修改仍失败，请刷新页面后重试",
            ),
            JsonRejection::JsonSyntaxError(_) => {
                ("REQUEST_BODY_INVALID", "提交资料无法读取，请刷新页面后重试；如仍失败，请联系支持人员")
            },
            JsonRejection::MissingJsonContentType(_) => (
                "REQUEST_CONTENT_TYPE_INVALID",
                "提交格式不受支持，请刷新页面后重试；如仍失败，请联系支持人员",
            ),
            _ => ("REQUEST_BODY_UNAVAILABLE", "提交资料无法接收，请检查文件大小后重试"),
        };
        let fields =
            json_path(&error).map(|path| BTreeMap::from([(path, "必填资料缺失或格式不正确".into())]));
        Self { status, code, message, fields }
    }

    fn query(error: QueryRejection) -> Self {
        Self {
            status: error.status(),
            code: "QUERY_INVALID",
            message: "查询条件格式不正确，请重置筛选后重新查询；如仍失败，请联系支持人员",
            fields: None,
        }
    }
}

fn json_path(error: &(dyn StdError + 'static)) -> Option<String> {
    if let Some(error) = error.downcast_ref::<JsonPathError<JsonError>>() {
        let mut path = error.path().to_string();
        if path == "." {
            path.clear();
        }
        // 只提取 serde 生成的字段名称；不向外复制非法值、枚举值或完整错误正文。
        let detail = error.inner().to_string();
        if let Some(field) = detail.strip_prefix("missing field `").and_then(|value| value.split('`').next())
            && field.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        {
            path = if path.is_empty() { field.to_string() } else { format!("{path}.{field}") };
        }
        return (!path.is_empty()).then_some(path);
    }
    error.source().and_then(json_path)
}

impl IntoResponse for RequestError {
    fn into_response(self) -> Response {
        tracing::warn!(target: "http_error", code = self.code, status = self.status.as_u16(), "Request decoding rejected");
        ApiResponse::<()> {
            status: self.status().as_u16(),
            message: self.message.into(),
            code: Some(self.code.into()),
            required_permission: None,
            field_errors: self.fields,
            retryable: Some(false),
            data: None,
            success: false,
        }
        .into_response()
    }
}

#[cfg(test)]
mod tests {
    use axum::body::{Body, to_bytes};
    use erp_processes::supplier_catalog_adoption::SaveBatchRequest;
    use serde::Deserialize;
    use serde_json::json;

    use super::*;

    #[derive(Debug, Deserialize)]
    struct Input {
        count: u32,
    }

    #[tokio::test]
    async fn json_success_and_invalid_field_keep_contract() {
        let request = |body| {
            Request::builder().header("content-type", "application/json").body(Body::from(body)).unwrap()
        };
        assert_eq!(Json::<Input>::from_request(request(r#"{"count":2}"#), &()).await.unwrap().0.count, 2);
        for body in [r#"{"count":"private-value"}"#, "{}"] {
            let response = Json::<Input>::from_request(request(body), &()).await.unwrap_err().into_response();
            assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
            let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
            let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(json["code"], "REQUEST_FIELD_INVALID");
            assert!(json["fieldErrors"]["count"].is_string());
            assert!(!String::from_utf8_lossy(&bytes).contains("private-value"));
        }
    }

    #[tokio::test]
    async fn batch_pricing_accepts_incomplete_inactive_input_but_rejects_invalid_active_prices() {
        let mut payload = json!({
            "expected_version":1,
            "rules":{"mode":"SAME","rule":{"method":"FIXED","value":"200"},"minimum_quantity":"100"},
            "inactive_pricing":{
                "mode":"SEPARATE",
                "dropship":{"method":"FIXED","value":"200"},
                "bulk":{"method":"PERCENTAGE","value":""},
                "minimum_quantity":""
            },
            "step":"DETAILS"
        });
        let request = |body: String| {
            Request::builder().header("content-type", "application/json").body(Body::from(body)).unwrap()
        };
        let decoded =
            Json::<SaveBatchRequest>::from_request(request(payload.to_string()), &()).await.unwrap().0;
        assert_eq!(decoded.step.as_deref(), Some("DETAILS"));
        assert!(decoded.inactive_pricing.is_some());

        for invalid in [json!(""), json!("200.001"), json!(200)] {
            payload["rules"]["rule"]["value"] = invalid;
            let response = Json::<SaveBatchRequest>::from_request(request(payload.to_string()), &())
                .await
                .unwrap_err()
                .into_response();
            assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
            let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
            let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(body["code"], "REQUEST_FIELD_INVALID");
            assert!(body["fieldErrors"]["rules"].is_string());
        }
    }

    #[test]
    fn invalid_query_is_not_an_unstructured_response() {
        let response =
            Query::<Input>::try_from_uri(&"/?count=bad".parse().unwrap()).unwrap_err().into_response();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert_eq!(response.headers()["content-type"], "application/json");
    }
}
