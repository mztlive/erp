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
        settings.validate().map_err(|_| mapping("京东连接设置不完整，请联系管理员核对接入配置"))?;
        if !settings.enabled {
            return Err(mapping("京东连接尚未启用，请联系管理员开通"));
        }
        let context = JdVopRequestContext::new(settings.source_type)?;
        let sender = ReqwestSender::new(settings.timeout_seconds)
            .map_err(|_| mapping("京东连接无法建立，请联系管理员核对接入配置"))?;
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
                "暂时无法连接京东，请稍后重新查询",
            ),
            HttpFailure::Oversized => mapping("京东返回的资料过多，无法读取，请缩小查询范围或联系管理员核对"),
        })?;
        if reply.body.len() > RESPONSE_LIMIT {
            return Err(mapping("京东返回的资料过多，无法读取，请缩小查询范围或联系管理员核对"));
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
        let summary = match class {
            SupplierFailureClass::AuthSignature => "京东未授权当前账号，请联系管理员核对连接授权",
            SupplierFailureClass::RateLimited => "京东当前查询较多，请稍后重新查询",
            SupplierFailureClass::TransientFailure => "暂时无法从京东读取资料，请稍后重新查询",
            _ => "京东未返回可读取的资料，请联系管理员核对连接配置",
        };
        Err(error(class, &code, summary))
    }
}

fn unknown_write(failure: ConnectorError) -> ConnectorError {
    error(
        SupplierFailureClass::ResultUnknown,
        &failure.code,
        "京东尚未确认原操作结果，请查询原订单或原操作，不要重复提交",
    )
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
