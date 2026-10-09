use std::sync::Arc;
use std::time::{Duration, Instant as Clock};

use async_trait::async_trait;
use config::DangaoshushuConfig;
use erp_core::common::time::Instant;
use erp_supply::entity::failure::SupplierFailureClass;
use erp_supply::ports::connector::common::{ConnectorError, ConnectorResult};
use reqwest::multipart::Form;
use reqwest::redirect::Policy;
use reqwest::{Client, RequestBuilder, retry};
use serde_json::Value;
use tokio::sync::Mutex;
use tokio::time::{sleep, timeout};

use super::{error, signing, wire};

const MAX_RESPONSE_BYTES: usize = 4 * 1024 * 1024;

pub(super) enum Request {
    Read {
        path: &'static str,
        parameters: Vec<(String, String)>,
    },
    Multipart {
        path: &'static str,
        parameters: Vec<(String, String)>,
        write: bool,
        not_after: Option<Instant>,
    },
    Json {
        path: &'static str,
        payload: Value,
    },
    Form {
        path: &'static str,
        parameters: Vec<(String, String)>,
        write: bool,
    },
}

impl Request {
    fn deadline(&self) -> Option<Instant> {
        match self {
            Self::Multipart { not_after, .. } => *not_after,
            _ => None,
        }
    }
    fn is_write(&self) -> bool {
        matches!(
            self,
            Self::Json { .. } | Self::Multipart { write: true, .. } | Self::Form { write: true, .. }
        )
    }
}

#[async_trait]
pub(super) trait Transport: Send + Sync {
    async fn send(&self, request: Request) -> ConnectorResult<Value>;
}

pub(super) struct HttpTransport {
    settings: DangaoshushuConfig,
    client: Client,
    quota: Arc<Mutex<(Clock, u32)>>,
}

impl HttpTransport {
    pub(super) fn new(settings: &DangaoshushuConfig) -> ConnectorResult<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(settings.timeout_seconds))
            .connect_timeout(Duration::from_secs(settings.timeout_seconds.min(5)))
            .redirect(Policy::none())
            .retry(retry::never())
            .build()
            .map_err(|_| {
                error(SupplierFailureClass::MappingError, "DGSS_HTTP_CONFIG", "供应商 HTTP 客户端配置无效")
            })?;
        Ok(Self { settings: settings.clone(), client, quota: Arc::new(Mutex::new((Clock::now(), 0))) })
    }

    async fn admission(&self) -> ConnectorResult<()> {
        timeout(Duration::from_secs(self.settings.timeout_seconds), self.await_admission()).await.map_err(
            |_| {
                let mut failure = error(
                    SupplierFailureClass::RateLimited,
                    "DGSS_LOCAL_RATE_LIMIT",
                    "供应商连接请求等待配额超时",
                );
                failure.retry_after = Some(Duration::from_secs(1));
                failure
            },
        )
    }

    async fn await_admission(&self) {
        let mut quota = self.quota.lock().await;
        if quota.0.elapsed() >= Duration::from_secs(1) {
            *quota = (Clock::now(), 0);
        }
        if quota.1 >= self.settings.requests_per_second {
            sleep(Duration::from_secs(1).saturating_sub(quota.0.elapsed())).await;
            *quota = (Clock::now(), 0);
        }
        quota.1 += 1;
    }

    fn signed_request(&self, request: Request) -> RequestBuilder {
        let timestamp = signing::timestamp(self.settings.timestamp_unit);
        let auth = [
            ("channel_no", self.settings.channel_no.clone()),
            ("timestamp", timestamp.clone()),
            ("sign", signing::sign(&self.settings.channel_no, &timestamp, &self.settings.private_key)),
        ];
        let endpoint = |path: &str| format!("{}{path}", self.settings.base_url.trim_end_matches('/'));
        match request {
            Request::Read { path, parameters } => {
                self.client.get(endpoint(path)).query(&auth).query(&parameters)
            },
            Request::Multipart { path, parameters, .. } => {
                let form =
                    parameters.into_iter().fold(Form::new(), |form, (key, value)| form.text(key, value));
                self.client.post(endpoint(path)).query(&auth).multipart(form)
            },
            Request::Json { path, mut payload } => {
                for (name, value) in auth {
                    payload[name] = Value::String(value);
                }
                self.client.post(endpoint(path)).json(&payload)
            },
            Request::Form { path, parameters, .. } => {
                self.client.post(endpoint(path)).query(&auth).form(&parameters)
            },
        }
    }
}

#[async_trait]
impl Transport for HttpTransport {
    async fn send(&self, request: Request) -> ConnectorResult<Value> {
        self.admission().await?;
        let remaining = sending_window(request.deadline(), Instant::now())?;
        let write = request.is_write();
        let mut builder = self.signed_request(request);
        if let Some(remaining) = remaining {
            builder = builder.timeout(remaining.min(Duration::from_secs(self.settings.timeout_seconds)));
        }
        let mut response = builder.send().await.map_err(|_| transport_failure(write))?;
        let status = response.status();
        if !status.is_success() {
            return Err(http_failure(status.as_u16(), write));
        }
        let mut body = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|_| transport_failure(write))? {
            if body.len().saturating_add(chunk.len()) > MAX_RESPONSE_BYTES {
                return Err(invalid_response(write));
            }
            body.extend_from_slice(&chunk);
        }
        decode(&body, write)
    }
}

fn sending_window(deadline: Option<Instant>, now: Instant) -> ConnectorResult<Option<Duration>> {
    let Some(deadline) = deadline else { return Ok(None) };
    if deadline <= now {
        return Err(error(
            SupplierFailureClass::BusinessRejected,
            "DGSS_DELIVERY_EXPIRED",
            "配送选项已过期，须重新查询后提交",
        ));
    }
    let seconds = deadline.unix_secs().checked_sub(now.unix_secs()).ok_or_else(|| invalid_response(false))?;
    let seconds = u64::try_from(seconds).map_err(|_| invalid_response(false))?;
    if seconds == 0 {
        return Err(error(
            SupplierFailureClass::BusinessRejected,
            "DGSS_DELIVERY_EXPIRED",
            "配送选项已过期，须重新查询后提交",
        ));
    }
    Ok(Some(Duration::from_secs(seconds)))
}

fn transport_failure(write: bool) -> ConnectorError {
    error(
        if write { SupplierFailureClass::ResultUnknown } else { SupplierFailureClass::TransientFailure },
        "DGSS_TRANSPORT",
        "供应商调用未取得可核实结果；写入须调查原动作",
    )
}
fn invalid_response(write: bool) -> ConnectorError {
    error(
        if write { SupplierFailureClass::ResultUnknown } else { SupplierFailureClass::MappingError },
        "DGSS_RESPONSE_INVALID",
        "供应商响应格式或大小不符合合同",
    )
}
fn http_failure(status: u16, write: bool) -> ConnectorError {
    let class = match status {
        401 | 403 => SupplierFailureClass::AuthSignature,
        _ if write => SupplierFailureClass::ResultUnknown,
        429 => SupplierFailureClass::RateLimited,
        _ => SupplierFailureClass::TransientFailure,
    };
    error(class, &format!("DGSS_HTTP_{status}"), "供应商返回 HTTP 失败")
}

pub(super) fn decode(body: &[u8], write: bool) -> ConnectorResult<Value> {
    let envelope = wire::parse(body).map_err(|_| invalid_response(write))?;
    let code = envelope
        .get("code")
        .and_then(|value| value.as_i64().or_else(|| value.as_str()?.parse().ok()))
        .ok_or_else(|| invalid_response(write))?;
    if code != 200 {
        let class = match code {
            201 | 90000 | 90001 | 90002 => SupplierFailureClass::AuthSignature,
            _ => SupplierFailureClass::BusinessRejected,
        };
        // msg 可能含地址、手机号或供应商回显的请求，不向日志/错误转发。
        return Err(error(class, &format!("DGSS_{code}"), "供应商明确拒绝请求"));
    }
    envelope.get("data").cloned().ok_or_else(|| invalid_response(write))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn expired_delivery_after_quota_wait_is_rejected_before_http_send() {
        let expires = Instant::from_unix_secs(1700000000);
        assert_eq!(
            sending_window(Some(expires), expires).unwrap_err().class,
            SupplierFailureClass::BusinessRejected
        );
        assert_eq!(
            sending_window(Some(expires), Instant::from_unix_secs(1699999999)).unwrap(),
            Some(Duration::from_secs(1))
        );
        assert_eq!(sending_window(None, expires).unwrap(), None);
    }
    #[tokio::test]
    async fn quota_waits_for_next_window_instead_of_rejecting_sixth_item() {
        let transport = HttpTransport::new(&super::super::test_support::settings()).unwrap();
        for _ in 0..6 {
            transport.admission().await.unwrap();
        }
        assert_eq!(transport.quota.lock().await.1, 1);
    }
    #[test]
    fn business_errors_and_malformed_write_results_are_distinct() {
        assert_eq!(
            decode(br#"{"code":90002,"msg":"secret-phone"}"#, false).unwrap_err().class,
            SupplierFailureClass::AuthSignature
        );
        assert_eq!(
            decode(br#"{"code":3005}"#, true).unwrap_err().class,
            SupplierFailureClass::BusinessRejected
        );
        assert_eq!(decode(b"invalid", true).unwrap_err().class, SupplierFailureClass::ResultUnknown);
        assert_eq!(decode(br#"{"code":200}"#, true).unwrap_err().class, SupplierFailureClass::ResultUnknown);
        assert!(
            !decode(br#"{"code":201,"msg":"secret-phone"}"#, false)
                .unwrap_err()
                .to_string()
                .contains("secret-phone")
        );
        assert_eq!(http_failure(502, true).class, SupplierFailureClass::ResultUnknown);
    }
}
