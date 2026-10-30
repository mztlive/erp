//! 将类型化业务请求、受控令牌和官方签名连接到单次 HTTP 发送，不自行启用生产账号。
use std::collections::BTreeMap;
use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, FixedOffset, Utc};
use config::JdVopConfig;
use erp_core::common::time::Instant;
use erp_supply::entity::failure::SupplierFailureClass;
use erp_supply::ports::connector::common::{ConnectorError, ConnectorResult};
use sender::ReqwestSender;
use serde_json::value::RawValue;

use super::request_encoding::{JdVopRequestContext, read_payload, write_payload};
use super::signing::encode_raw;
use super::token::{JdVopTokenProvider, token_error};
use super::transport::{JdVopReadRequest, JdVopReadTransport, JdVopWriteRequest, JdVopWriteTransport};
use super::{error, mapping};

mod sender;
#[cfg(test)]
mod tests;
const RESPONSE_LIMIT: usize = 8 * 1024 * 1024;

struct HttpReply {
    status: u16,
    body: Vec<u8>,
}
enum HttpFailure {
    Transport,
    Oversized,
}
#[async_trait]
trait HttpSender: Send + Sync {
    async fn send(&self, endpoint: &str, encoded: String) -> Result<HttpReply, HttpFailure>;
}
trait Clock: Send + Sync {
    fn now(&self) -> Instant;
}
struct SystemClock;
impl Clock for SystemClock {
    fn now(&self) -> Instant {
        Instant::now()
    }
}

/// 独立、受信注入的京东 HTTP 传输；不接入业务配置的自动启用开关，不提供 Debug。
pub struct JdVopHttpTransport {
    settings: JdVopConfig,
    context: JdVopRequestContext,
    tokens: Arc<dyn JdVopTokenProvider>,
    sender: Arc<dyn HttpSender>,
    clock: Arc<dyn Clock>,
}

impl JdVopHttpTransport {
    /// 构造可供协议运行时显式注入的 HTTP 客户端，不发起网络。
    /// # 参数
    /// `settings/tokens` 来自受控装配；渠道取自配置，时间按本应用固定 UTC+8 政策编码。
    /// # 返回
    /// 禁止重定向且每次调用仅一次发送的客户端，不代表真实账号验收。
    /// # 错误
    /// 配置、渠道或 HTTP 客户端构造无效时拒绝。
    pub fn new(settings: JdVopConfig, tokens: Arc<dyn JdVopTokenProvider>) -> ConnectorResult<Self> {
        settings.validate().map_err(|_| mapping("京东 HTTP 技术配置无效"))?;
        if !settings.enabled {
            return Err(mapping("京东 HTTP 尚未启用"));
        }
        let context = JdVopRequestContext::new(settings.source_type)?;
        let sender =
            ReqwestSender::new(settings.timeout_seconds).map_err(|_| mapping("京东 HTTP 客户端无法创建"))?;
        Ok(Self { settings, context, tokens, sender: Arc::new(sender), clock: Arc::new(SystemClock) })
    }

    async fn execute(&self, method: &str, payload: Box<RawValue>, write: bool) -> ConnectorResult<Vec<u8>> {
        let token =
            self.tokens.access_token(&self.settings.credential_id).await.map_err(|_| token_error())?;
        let now = self.clock.now();
        let access_token = token.value_for(&self.settings.account_scope, now)?;
        let timestamp = DateTime::<Utc>::from_timestamp(now.unix_secs(), 0)
            .ok_or_else(|| mapping("京东请求时间无法编码"))?
            // 本应用采用固定 UTC+8，不从宿主机时区或账号参数推导。
            .with_timezone(&FixedOffset::east_opt(8 * 3600).expect("固定UTC+8位于合法时区范围"))
            .format("%Y-%m-%d %H:%M:%S")
            .to_string();
        let parameters = BTreeMap::from([
            ("method".into(), method.into()),
            ("app_key".into(), self.settings.app_key.clone()),
            ("access_token".into(), access_token.into()),
            ("timestamp".into(), timestamp),
            ("format".into(), "json".into()),
            ("v".into(), "2.0".into()),
        ]);
        let encoded = encode_raw(parameters, payload.as_ref(), &self.settings.app_secret)?;
        let reply = self.sender.send(&self.settings.gateway_url, encoded.into_string()).await;
        Self::response(reply).map_err(|failure| if write { unknown_write(failure) } else { failure })
    }

    fn response(reply: Result<HttpReply, HttpFailure>) -> ConnectorResult<Vec<u8>> {
        let reply = reply.map_err(|failure| match failure {
            HttpFailure::Transport => error(
                SupplierFailureClass::TransientFailure,
                "JD_VOP_HTTP_TRANSPORT_FAILED",
                "京东网络请求失败，请稍后核对原结果",
            ),
            HttpFailure::Oversized => mapping("京东响应超过允许大小"),
        })?;
        if reply.body.len() > RESPONSE_LIMIT {
            return Err(mapping("京东响应超过允许大小"));
        }
        let status = reply.status;
        if (200..300).contains(&status) {
            return Ok(reply.body);
        }
        let class = match status {
            401 | 403 => SupplierFailureClass::AuthSignature,
            429 => SupplierFailureClass::RateLimited,
            500..=599 => SupplierFailureClass::TransientFailure,
            _ => SupplierFailureClass::MappingError,
        };
        let code = if (300..400).contains(&status) {
            "JD_VOP_HTTP_REDIRECT_REJECTED".into()
        } else {
            format!("JD_VOP_HTTP_{status}")
        };
        Err(error(class, &code, "京东 HTTP 请求未成功，请核对授权、配额或原请求结果"))
    }
}

fn unknown_write(failure: ConnectorError) -> ConnectorError {
    error(SupplierFailureClass::ResultUnknown, &failure.code, "京东原写请求结果待核实，请查询原结果")
}

#[async_trait]
impl JdVopReadTransport for JdVopHttpTransport {
    async fn read(&self, request: &JdVopReadRequest) -> ConnectorResult<Vec<u8>> {
        self.execute(request.method(), read_payload(request, &self.context)?, false).await
    }
}
#[async_trait]
impl JdVopWriteTransport for JdVopHttpTransport {
    async fn write(&self, request: &JdVopWriteRequest) -> ConnectorResult<Vec<u8>> {
        self.execute(request.method(), write_payload(request, &self.context)?, true).await
    }
}
