use std::collections::VecDeque;
use std::sync::Mutex;

use serde_json::{Value, from_str};
use url::form_urlencoded::parse;

use super::super::token::JdVopAccessToken;
use super::*;

struct FixedClock;
impl Clock for FixedClock {
    fn now(&self) -> Instant {
        Instant::from_unix_secs(1_700_000_000)
    }
}
struct Tokens {
    fail: bool,
    scope: String,
    expires: i64,
    references: Mutex<Vec<String>>,
}
#[async_trait]
impl JdVopTokenProvider for Tokens {
    async fn access_token(&self, reference: &str) -> ConnectorResult<JdVopAccessToken> {
        self.references.lock().unwrap().push(reference.into());
        if self.fail {
            return Err(error(
                SupplierFailureClass::AuthSignature,
                "PRIVATE",
                "private-token-provider-message",
            ));
        }
        JdVopAccessToken::new(
            "access-token".into(),
            "version-1".into(),
            self.scope.clone(),
            Instant::from_unix_secs(self.expires),
        )
    }
}
struct Sender {
    calls: Mutex<Vec<(String, String)>>,
    replies: Mutex<VecDeque<Result<HttpReply, HttpFailure>>>,
}
#[async_trait]
impl HttpSender for Sender {
    async fn send(&self, endpoint: &str, encoded: String) -> Result<HttpReply, HttpFailure> {
        self.calls.lock().unwrap().push((endpoint.into(), encoded));
        self.replies.lock().unwrap().pop_front().unwrap_or(Err(HttpFailure::Transport))
    }
}
fn fixture(
    fail: bool,
    scope: &str,
    expires: i64,
    replies: Vec<Result<HttpReply, HttpFailure>>,
) -> (JdVopHttpTransport, Arc<Sender>) {
    fixture_source(None, fail, scope, expires, replies)
}
fn fixture_source(
    source_type: Option<u32>,
    fail: bool,
    scope: &str,
    expires: i64,
    replies: Vec<Result<HttpReply, HttpFailure>>,
) -> (JdVopHttpTransport, Arc<Sender>) {
    let mut configuration = serde_json::json!({"enabled":true,"gateway_url":"https://gateway.example/router","app_key":"app","app_secret":"secret","credential_id":"jd-account-token","credential_encryption_key":"offline-test-only-independent-key-at-least-32","account_scope":"account"});
    if let Some(source_type) = source_type {
        configuration["source_type"] = serde_json::json!(source_type);
    }
    let settings: JdVopConfig = serde_json::from_value(configuration).unwrap();
    let tokens = Arc::new(Tokens { fail, scope: scope.into(), expires, references: Mutex::new(vec![]) });
    let sender = Arc::new(Sender { calls: Mutex::new(vec![]), replies: Mutex::new(replies.into()) });
    let mut transport = JdVopHttpTransport::new(settings, tokens).unwrap();
    transport.sender = sender.clone();
    transport.clock = Arc::new(FixedClock);
    (transport, sender)
}
fn reply(status: u16) -> Result<HttpReply, HttpFailure> {
    Ok(HttpReply { status, body: b"documented-response".to_vec() })
}

#[tokio::test]
async fn real_dispatch_sends_exact_signed_business_form_once() {
    let (transport, sender) = fixture(false, "account", 1_800_000_000, vec![reply(200)]);
    assert_eq!(transport.read(&JdVopReadRequest::Pools).await.unwrap(), b"documented-response");
    let calls = sender.calls.lock().unwrap();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].0, "https://gateway.example/router");

    let parameters: BTreeMap<_, _> = parse(calls[0].1.as_bytes()).into_owned().collect();
    // 独立 Python hashlib/urllib 向量，JSON原值与发送表单共用同一次序列化。
    assert_eq!(parameters["sign"], "CA40A136486722CF947142BE0F3AC9B5");
    assert_eq!(parameters["360buy_param_json"], r#"{"getSkuPoolInfoGoodsReq":{"sourceType":3}}"#);
    assert_eq!(parameters["timestamp"], "2023-11-15 06:13:20");
    assert_eq!(parameters["access_token"], "access-token");
    let business: Value = from_str(&parameters["360buy_param_json"]).unwrap();
    assert_eq!(business["getSkuPoolInfoGoodsReq"]["sourceType"], 3);
    assert_eq!(parameters["app_key"], "app");
    assert_eq!(
        calls[0].1,
        "360buy_param_json=%7B%22getSkuPoolInfoGoodsReq%22%3A%7B%22sourceType%22%3A3%7D%7D&access_token=access-token&app_key=app&format=json&method=jingdong.vop.goods.getSkuPoolInfo&sign=CA40A136486722CF947142BE0F3AC9B5&timestamp=2023-11-15+06%3A13%3A20&v=2.0"
    );
    assert!(!parameters.contains_key("app_secret"));
    assert!(!calls[0].1.contains("secret"));
}

#[tokio::test]
async fn pool_page_dispatch_signs_and_sends_top_level_pagination_once() {
    let (transport, sender) = fixture(false, "account", 1_800_000_000, vec![reply(200)]);
    let request = JdVopReadRequest::PoolPage {
        pool_id: "pool".into(),
        offset: "0".into(),
        page_size: 20.try_into().unwrap(),
    };
    transport.read(&request).await.unwrap();
    let calls = sender.calls.lock().unwrap();
    assert_eq!(calls.len(), 1);
    let parameters: BTreeMap<_, _> = parse(calls[0].1.as_bytes()).into_owned().collect();
    assert_eq!(parameters["method"], "jingdong.vop.goods.querySkuByPage");
    assert_eq!(
        parameters["360buy_param_json"],
        r#"{"bizPoolId":"pool","offset":0,"pageSize":20,"sourceType":3}"#
    );
    // 独立 Python hashlib 向量，核验签名字节与实际发送的顶层业务 JSON 一致。
    assert_eq!(parameters["sign"], "98F6AA12E6F3E76D99AD2EF250C41BBD");
}

#[tokio::test]
async fn explicit_source_override_is_used_by_real_constructor_and_mismatch_sends_nothing() {
    let (transport, sender) = fixture_source(Some(7), false, "account", 1_800_000_000, vec![reply(200)]);
    assert!(transport.read(&JdVopReadRequest::FundsBalance { source_type: 7 }).await.is_ok());
    {
        let calls = sender.calls.lock().unwrap();
        let parameters: BTreeMap<_, _> = parse(calls[0].1.as_bytes()).into_owned().collect();
        assert_eq!(parameters["360buy_param_json"], r#"{"checkAccountBalanceOpenReq":{"sourceType":7}}"#);
    }

    assert!(transport.read(&JdVopReadRequest::FundsBalance { source_type: 3 }).await.is_err());
    assert_eq!(sender.calls.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn invalid_token_account_expiry_and_provider_failure_send_nothing_and_are_sanitized() {
    for (fail, scope, expires) in
        [(true, "account", 1_800_000_000), (false, "other", 1_800_000_000), (false, "account", 1_700_000_000)]
    {
        let (transport, sender) = fixture(fail, scope, expires, vec![]);
        let failure = transport.read(&JdVopReadRequest::Pools).await.unwrap_err();
        assert_eq!(failure.class, SupplierFailureClass::AuthSignature);
        assert!(!failure.summary.contains("private-token"));
        assert!(sender.calls.lock().unwrap().is_empty());
    }
}

#[tokio::test]
async fn redirect_auth_rate_limit_and_server_failure_are_classified_without_following_or_retry() {
    for (status, class) in [
        (302, SupplierFailureClass::MappingError),
        (401, SupplierFailureClass::AuthSignature),
        (403, SupplierFailureClass::AuthSignature),
        (429, SupplierFailureClass::RateLimited),
        (503, SupplierFailureClass::TransientFailure),
    ] {
        let (transport, sender) = fixture(false, "account", 1_800_000_000, vec![reply(status), reply(200)]);
        let failure = transport.read(&JdVopReadRequest::Pools).await.unwrap_err();
        assert_eq!(failure.class, class);
        assert_eq!(sender.calls.lock().unwrap().len(), 1);
        assert_eq!(sender.replies.lock().unwrap().len(), 1);
        if status == 302 {
            assert_eq!(failure.code, "JD_VOP_HTTP_REDIRECT_REJECTED");
        }
    }
}

#[tokio::test]
async fn writes_are_single_attempt_and_transport_or_response_limits_preserve_unknown_result() {
    for response in [
        Err(HttpFailure::Transport),
        Err(HttpFailure::Oversized),
        reply(500),
        Ok(HttpReply { status: 200, body: vec![b'x'; RESPONSE_LIMIT + 1] }),
    ] {
        let (transport, sender) = fixture(false, "account", 1_800_000_000, vec![response, reply(200)]);
        let request = JdVopWriteRequest::Confirm {
            merchant_order_no: "original".into(),
            external_order_no: "123".into(),
            source_type: 3,
        };
        let failure = transport.write(&request).await.unwrap_err();
        assert_eq!(failure.class, SupplierFailureClass::ResultUnknown);
        assert_eq!(sender.calls.lock().unwrap().len(), 1);
        assert_eq!(sender.replies.lock().unwrap().len(), 1);
    }

    let (transport, sender) = fixture(false, "account", 1_800_000_000, vec![]);
    let request = JdVopWriteRequest::Confirm {
        merchant_order_no: "original".into(),
        external_order_no: "123".into(),
        source_type: 4,
    };
    assert!(transport.write(&request).await.is_err());
    assert!(sender.calls.lock().unwrap().is_empty());
}
