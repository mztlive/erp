//! 固定官方OAuth地址的单次GET；禁跟随重定向、禁重试、禁输出URI及响应正文。
use std::time::Duration;

use async_trait::async_trait;
use config::JdVopConfig;
use erp_supply::entity::supplier_api::SupplierApiConnection;
use erp_supply::ports::connector::common::ConnectorResult;
use erp_supply::ports::jd_vop::authorization::{
    AuthorizationError, AuthorizationGrant, AuthorizationHttp, AuthorizationParameters,
    JdVopAuthorizationRequest, Result,
};
use reqwest::Client;
use reqwest::redirect::Policy;
use reqwest::retry::never;
use serde_json::{Value, from_slice};
use zeroize::Zeroizing;

use super::JdVopRuntime;
use super::authorization_protocol::{
    AUTHORIZE_ENDPOINT, TOKEN_ENDPOINT, authorization_code, exchange_parameters, prepare,
};
use super::oauth::parse_refresh;

const RESPONSE_LIMIT: usize = 1024 * 1024;

pub struct ReqwestAuthorizationHttp {
    client: Result<Client>,
    binding: ConnectorResult<JdVopRuntime>,
}

impl ReqwestAuthorizationHttp {
    /// 以当前启动配置创建有界授权通信与连接绑定核验器。
    ///
    /// # 参数
    /// `settings` 为同一部署的京东配置快照。
    ///
    /// # 返回
    /// 供应用注入编排的授权技术实现。
    ///
    /// # 错误
    /// 构造不返回错误；通信错误由 `ensure_available` 返回，绑定核验保持独立。
    pub fn new(settings: &JdVopConfig) -> Self {
        let client = Client::builder()
            .redirect(Policy::none())
            .retry(never())
            .timeout(Duration::from_secs(settings.timeout_seconds))
            .build()
            .map_err(|_| AuthorizationError::Unavailable);
        Self { client, binding: JdVopRuntime::new(settings.clone()) }
    }

    async fn get(&self, endpoint: &'static str, parameters: &str) -> Result<Reply> {
        let client = self.client.as_ref().map_err(|error| *error)?;
        let url = Zeroizing::new(format!("{endpoint}?{parameters}"));
        let mut response = client.get(url.as_str()).send().await.map_err(|_| AuthorizationError::Unknown)?;

        let status = response.status().as_u16();
        let location = response
            .headers()
            .get("location")
            .and_then(|value| value.to_str().ok())
            .map(|value| Zeroizing::new(value.to_string()));
        if (300..400).contains(&status) {
            return Ok(Reply { status, location, body: Zeroizing::new(Vec::new()) });
        }

        if response.content_length().is_some_and(|size| size > 1_048_576) {
            return Err(AuthorizationError::Unknown);
        }
        let mut body = Zeroizing::new(Vec::new());
        while let Some(chunk) = response.chunk().await.map_err(|_| AuthorizationError::Unknown)? {
            if body.len().checked_add(chunk.len()).is_none_or(|size| size > RESPONSE_LIMIT) {
                return Err(AuthorizationError::Unknown);
            }
            body.extend_from_slice(&chunk);
        }
        Ok(Reply { status, location, body })
    }
}

struct Reply {
    status: u16,
    location: Option<Zeroizing<String>>,
    body: Zeroizing<Vec<u8>>,
}

#[async_trait]
impl AuthorizationHttp for ReqwestAuthorizationHttp {
    fn ensure_available(&self) -> Result<()> {
        self.client.as_ref().map(|_| ()).map_err(|error| *error)
    }

    fn verify_binding(&self, connection: &SupplierApiConnection) -> Result<()> {
        self.binding
            .as_ref()
            .map_err(|_| AuthorizationError::BindingMismatch)?
            .verify_authorization_binding(connection)
            .map_err(|_| AuthorizationError::BindingMismatch)
    }

    fn prepare(&self, request: &JdVopAuthorizationRequest, app_key: &str) -> Result<AuthorizationParameters> {
        prepare(request, app_key)
    }
    fn exchange_parameters(&self, app_key: &str, app_secret: &str, code: &str) -> Zeroizing<String> {
        exchange_parameters(app_key, app_secret, code)
    }

    async fn code(&self, parameters: AuthorizationParameters) -> Result<Zeroizing<String>> {
        let reply = self.get(AUTHORIZE_ENDPOINT, &parameters.query).await?;
        if reply.status == 302 {
            let location = reply.location.as_ref().ok_or(AuthorizationError::Unknown)?;
            return authorization_code(&parameters.callback, location);
        }
        if explicitly_rejected(&reply.body) {
            return Err(AuthorizationError::Rejected);
        }
        Err(AuthorizationError::Unknown)
    }

    async fn exchange(&self, parameters: Zeroizing<String>) -> Result<AuthorizationGrant> {
        let reply = self.get(TOKEN_ENDPOINT, &parameters).await?;
        if !(200..300).contains(&reply.status) {
            return Err(AuthorizationError::Unknown);
        }
        grant(&reply.body)
    }
}

fn explicitly_rejected(body: &[u8]) -> bool {
    let Ok(value) = from_slice::<Value>(body) else { return false };
    let code = value.get("code").or_else(|| value.get("resultCode"));
    let code = code.and_then(|code| {
        code.as_str().map(str::to_string).or_else(|| code.as_i64().map(|value| value.to_string()))
    });
    code.is_some_and(|code| {
        matches!(
            code.as_str(),
            "10001"
                | "10002"
                | "10003"
                | "20001"
                | "20002"
                | "20003"
                | "20004"
                | "20005"
                | "20006"
                | "20007"
                | "20008"
                | "20009"
                | "20020"
                | "20025"
                | "1026"
                | "1027"
                | "302"
                | "304"
                | "305"
        )
    })
}

fn grant(body: &[u8]) -> Result<AuthorizationGrant> {
    if body.len() > RESPONSE_LIMIT {
        return Err(AuthorizationError::Unknown);
    }
    let tokens = parse_refresh(body).map_err(|_| AuthorizationError::Unknown)?;

    let root: Value = from_slice(body).map_err(|_| AuthorizationError::Unknown)?;
    let object = root.get("result").unwrap_or(&root);
    let origin = object.get("originJosResp").unwrap_or(object);
    let identity = origin
        .get("uid")
        .and_then(Value::as_str)
        .map(|id| ("uid", id))
        .or_else(|| {
            object
                .get("openId")
                .or_else(|| origin.get("open_id"))
                .and_then(Value::as_str)
                .map(|id| ("open_id", id))
        })
        .map(|(kind, id)| {
            if id.is_empty() || id.len() > 8192 || id.chars().any(char::is_control) {
                return Err(AuthorizationError::Unknown);
            }
            Ok(Zeroizing::new(format!("{kind}:{id}")))
        })
        .transpose()?;
    Ok(AuthorizationGrant { tokens, identity })
}

#[cfg(test)]
mod tests {
    use erp_core::ids::{SupplierAccountId, SupplierApiConnectionId};
    use erp_supply::entity::supplier_api::{
        SupplierApiConnectionData, SupplierApiConnectionStatus, SupplierConnectorType,
    };
    use erp_supply::ports::supplier_reference_registry::{
        SupplierReferenceRegistry, SupplierReferenceTarget,
    };

    use super::*;

    fn settings() -> JdVopConfig {
        serde_json::from_value(serde_json::json!({
            "enabled": true, "app_key": "app", "app_secret": "secret",
            "credential_id": "credential", "account_scope": "account",
            "credential_encryption_key": "offline-test-only-independent-key-at-least-32"
        }))
        .unwrap()
    }

    fn connection(runtime: &JdVopRuntime) -> SupplierApiConnection {
        let mut connection = SupplierApiConnection::new(
            SupplierApiConnectionId::new("connection"),
            SupplierApiConnectionData {
                supplier_id: SupplierAccountId::new("supplier"),
                connection_code: "jd".into(),
                connector_type: SupplierConnectorType::JdVop,
                endpoint_reference: String::new(),
                credential_reference: None,
                rate_limit_policy: None,
                status: SupplierApiConnectionStatus::Disabled,
            },
            "actor",
        )
        .unwrap();
        let references = runtime.deployment_config(&SupplierReferenceTarget::from(&connection)).unwrap();
        connection.endpoint_reference = references.endpoint;
        connection.credential_reference = Some(references.credential);
        connection.endpoint_reference_bound = true;
        connection.credential_reference_bound = true;
        connection
    }

    #[tokio::test]
    async fn unavailable_http_preserves_local_binding_and_preflight_but_rejects_communication() {
        let runtime = JdVopRuntime::new(settings()).unwrap();
        let mut connection = connection(&runtime);
        // Inject only HTTP initialization failure; exercise the production binding implementation.
        let protocol =
            ReqwestAuthorizationHttp { client: Err(AuthorizationError::Unavailable), binding: Ok(runtime) };
        assert_eq!(protocol.verify_binding(&connection), Ok(()));
        assert_eq!(protocol.ensure_available(), Err(AuthorizationError::Unavailable));

        let fixture: Value =
            serde_json::from_str(include_str!("authorization_protocol/rsa_offline_fixture.json")).unwrap();
        let request = JdVopAuthorizationRequest {
            operation_id: "11111111-1111-4111-8111-111111111111".into(),
            expected_version: None,
            username: Zeroizing::new("buyer".into()),
            password: Zeroizing::new("offline-test-password".into()),
            redirect_uri: "https://erp.example/callback".into(),
            private_key: Zeroizing::new(fixture["private_pem"].as_str().unwrap().into()),
            evidence_reference: None,
        };
        let parameters = protocol.prepare(&request, "app").unwrap();
        assert_eq!(protocol.code(parameters).await, Err(AuthorizationError::Unavailable));
        assert!(matches!(
            protocol.exchange(Zeroizing::new("code=offline".into())).await,
            Err(AuthorizationError::Unavailable)
        ));

        connection.credential_reference = Some("wrong-binding".into());
        assert_eq!(protocol.verify_binding(&connection), Err(AuthorizationError::BindingMismatch));
    }

    #[test]
    fn invalid_local_configuration_keeps_binding_error_when_http_is_unavailable() {
        let runtime = JdVopRuntime::new(settings()).unwrap();
        let connection = connection(&runtime);
        let mut disabled = settings();
        disabled.enabled = false;
        let protocol = ReqwestAuthorizationHttp {
            client: Err(AuthorizationError::Unavailable),
            binding: JdVopRuntime::new(disabled),
        };
        assert_eq!(protocol.verify_binding(&connection), Err(AuthorizationError::BindingMismatch));
    }

    #[test]
    fn only_documented_login_rejections_can_release_candidate_identity() {
        assert!(explicitly_rejected(br#"{"code":1026,"message":"private-password"}"#));
        assert!(!explicitly_rejected(br#"{"code":0}"#));
        assert!(!explicitly_rejected(br#"{"code":20018}"#));
        assert!(!explicitly_rejected(b"private-password"));
    }

    #[test]
    fn exchange_reads_remote_identity_but_never_exposes_token_body_in_errors() {
        let result =
            grant(br#"{"code":0,"uid":"remote-123","access_token":"a","refresh_token":"r","expires_in":60}"#)
                .unwrap();
        assert_eq!(result.identity.unwrap().as_str(), "uid:remote-123");
        assert!(grant(br#"{"code":20010,"message":"private-token"}"#).is_err());
    }
}
