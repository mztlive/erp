use std::collections::BTreeMap;

use rsa::pkcs1::EncodeRsaPrivateKey;
use rsa::pkcs8::{EncodePrivateKey, LineEnding};
use serde_json::Value;
use url::form_urlencoded::parse;

use super::*;

fn vector() -> Value {
    serde_json::from_str(include_str!("rsa_offline_fixture.json")).unwrap()
}
fn request() -> JdVopAuthorizationRequest {
    let vector = vector();
    JdVopAuthorizationRequest {
        operation_id: "11111111-1111-4111-8111-111111111111".into(),
        expected_version: None,
        username: Zeroizing::new("企业采购 账号&=中国".into()),
        password: Zeroizing::new(vector["password"].as_str().unwrap().into()),
        redirect_uri: "https://erp.example/callback?tenant=A%26B".into(),
        private_key: Zeroizing::new(vector["private_pem"].as_str().unwrap().into()),
        evidence_reference: None,
    }
}

#[test]
fn raw_private_rsa_matches_independent_openssl_vector_for_all_supported_key_encodings() {
    // 专门随机生成的离线测试RSA2048；期望值来自OpenSSL pkeyutl raw PKCS1签名，不含任何真实凭据。
    let vector = vector();
    let key = RsaPrivateKey::from_pkcs8_pem(vector["private_pem"].as_str().unwrap()).unwrap();
    let formats = [
        vector["private_pem"].as_str().unwrap().to_string(),
        key.to_pkcs1_pem(LineEnding::LF).unwrap().to_string(),
        STANDARD.encode(key.to_pkcs8_der().unwrap().as_bytes()),
    ];
    for format in formats {
        let encoded = password(vector["password"].as_str().unwrap(), &format).unwrap();
        assert_eq!(encoded.as_str(), vector["signature_base64"].as_str().unwrap());
    }
}

#[test]
fn authorization_query_preserves_unicode_password_cipher_and_registered_callback_once() {
    let request = request();
    let parameters = prepare(&request, "application").unwrap();
    let fields: BTreeMap<_, _> = parse(parameters.query.as_bytes()).into_owned().collect();
    assert_eq!(fields["app_key"], "application");
    assert_eq!(fields["username"], request.username.as_str());
    assert_eq!(fields["redirect_uri"], request.redirect_uri);
    assert_eq!(fields["password"], vector()["signature_base64"].as_str().unwrap());
    assert_eq!(fields["response_type"], "code");
    assert_eq!(fields["scope"], "snsapi_base");
    assert!(!parameters.query.contains("offline-test-password"));
}

#[test]
fn callback_matching_requires_original_origin_path_query_and_unique_code_without_visiting_callback() {
    let callback = callback_uri("http://127.0.0.1:8080/callback?tenant=A%26B").unwrap();
    let code = authorization_code(
        &callback,
        "http://127.0.0.1:8080/callback?tenant=A%26B&state=null&code=a%2B%26%3D",
    )
    .unwrap();
    assert_eq!(code.as_str(), "a+&=");

    for location in [
        "https://127.0.0.1:8080/callback?tenant=A%26B&code=x",
        "http://127.0.0.1:8081/callback?tenant=A%26B&code=x",
        "http://evil.example:8080/callback?tenant=A%26B&code=x",
        "http://127.0.0.1:8080/other?tenant=A%26B&code=x",
        "http://127.0.0.1:8080/callback?tenant=other&code=x",
        "http://127.0.0.1:8080/callback?tenant=A%26B&code=x&code=y",
        "http://127.0.0.1:8080/callback?tenant=A%26B&code=",
        "http://127.0.0.1:8080/callback?tenant=A%26B&code=x#fragment",
        "http://127.0.0.1:8080/callback?tenant=A%26B&code=x&unexpected=1",
    ] {
        assert!(authorization_code(&callback, location).is_err());
    }
}

#[test]
fn invalid_key_oversized_input_and_callback_errors_are_static() {
    let mut invalid = request();
    invalid.private_key = Zeroizing::new("private-key-sentinel".into());
    let error = prepare(&invalid, "app").err().unwrap();
    assert_eq!(error, AuthorizationError::InvalidInput);
    assert!(!error.to_string().contains("private-key-sentinel"));

    let mut oversized = request();
    oversized.private_key = Zeroizing::new("x".repeat(32769));
    assert!(oversized.validate().is_err());

    let mut username = request();
    username.username = Zeroizing::new("中".repeat(86));
    assert!(username.validate().is_err());

    for value in [
        "javascript:alert(1)",
        "https://user:password@erp.example/callback",
        "https://erp.example/callback#fragment",
        "https://erp.example/callback?code=old",
    ] {
        assert!(callback_uri(value).is_err());
    }
}
