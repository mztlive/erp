//! 官方授权指南及Java demo的RSA私钥操作；callback只作结构核验，不访问其地址。
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use erp_supply::ports::jd_vop::authorization::{
    AuthorizationError, AuthorizationParameters, JdVopAuthorizationRequest, Result, callback_uri,
};
use rsa::pkcs1::DecodeRsaPrivateKey;
use rsa::pkcs8::DecodePrivateKey;
use rsa::rand_core::OsRng;
use rsa::traits::PublicKeyParts;
use rsa::{Pkcs1v15Sign, RsaPrivateKey};
use url::Url;
use url::form_urlencoded::Serializer;
use zeroize::Zeroizing;

pub(super) const AUTHORIZE_ENDPOINT: &str = "https://open-oauth.jd.com/oauth2/authorizeForVOP";
pub(super) const TOKEN_ENDPOINT: &str = "https://open-oauth.jd.com/oauth2/access_token";

pub(super) fn prepare(request: &JdVopAuthorizationRequest, app_key: &str) -> Result<AuthorizationParameters> {
    request.validate()?;
    let encrypted = password(&request.password, &request.private_key)?;
    let query = Serializer::new(String::new())
        .extend_pairs([
            ("app_key", app_key),
            ("response_type", "code"),
            ("redirect_uri", &request.redirect_uri),
            ("username", request.username.as_str()),
            ("password", encrypted.as_str()),
            ("scope", "snsapi_base"),
        ])
        .finish();
    Ok(AuthorizationParameters {
        query: Zeroizing::new(query),
        callback: callback_uri(&request.redirect_uri)?,
    })
}

fn password(password: &str, private_key: &str) -> Result<Zeroizing<String>> {
    let key = private_key.trim();
    let key = if key.starts_with("-----BEGIN") {
        RsaPrivateKey::from_pkcs8_pem(key)
            .or_else(|_| RsaPrivateKey::from_pkcs1_pem(key))
            .map_err(|_| AuthorizationError::InvalidInput)?
    } else {
        let compact = Zeroizing::new(key.chars().filter(|ch| !ch.is_ascii_whitespace()).collect::<String>());
        let der = Zeroizing::new(
            STANDARD.decode(compact.as_bytes()).map_err(|_| AuthorizationError::InvalidInput)?,
        );
        RsaPrivateKey::from_pkcs8_der(&der).map_err(|_| AuthorizationError::InvalidInput)?
    };
    if !(1024..=4096).contains(&key.n().bits()) || key.validate().is_err() {
        return Err(AuthorizationError::InvalidInput);
    }
    let digest = Zeroizing::new(format!("{:x}", md5::compute(password.as_bytes())));
    // Java Cipher("RSA").ENCRYPT_MODE(privateKey): type1 PKCS#1 v1.5，原值是32字节小写MD5 ASCII，无DigestInfo。
    let result = key
        .sign_with_rng(&mut OsRng, Pkcs1v15Sign::new_unprefixed(), digest.as_bytes())
        .map_err(|_| AuthorizationError::InvalidInput)?;
    Ok(Zeroizing::new(STANDARD.encode(result)))
}

pub(super) fn authorization_code(callback: &Url, location: &str) -> Result<Zeroizing<String>> {
    if location.len() > 8192 {
        return Err(AuthorizationError::Unknown);
    }
    let mut target = Url::parse(location).map_err(|_| AuthorizationError::Unknown)?;
    if target.fragment().is_some() {
        return Err(AuthorizationError::Unknown);
    }
    let mut extras = Vec::new();
    let mut code = None;
    let mut state = false;
    for (name, value) in target.query_pairs() {
        match name.as_ref() {
            "code"
                if code.is_none()
                    && !value.is_empty()
                    && value.len() <= 4096
                    && !value.chars().any(char::is_control) =>
            {
                code = Some(Zeroizing::new(value.into_owned()))
            },
            "state" if !state => state = true,
            "code" | "state" | "error" => return Err(AuthorizationError::Unknown),
            _ => extras.push((name.into_owned(), value.into_owned())),
        }
    }
    let expected: Vec<_> =
        callback.query_pairs().map(|(key, value)| (key.into_owned(), value.into_owned())).collect();
    if extras != expected {
        return Err(AuthorizationError::Unknown);
    }
    target.set_query(callback.query());
    if target != *callback {
        return Err(AuthorizationError::Unknown);
    }
    code.ok_or(AuthorizationError::Unknown)
}

pub(super) fn exchange_parameters(app_key: &str, app_secret: &str, code: &str) -> Zeroizing<String> {
    Zeroizing::new(
        Serializer::new(String::new())
            .extend_pairs([
                ("app_key", app_key),
                ("app_secret", app_secret),
                ("grant_type", "authorization_code"),
                ("code", code),
            ])
            .finish(),
    )
}

#[cfg(test)]
mod tests;
