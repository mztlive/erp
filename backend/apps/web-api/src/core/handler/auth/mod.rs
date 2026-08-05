pub mod consumer;
pub mod login;
pub mod profile;

use entities::LoginAccount;
use std::net::IpAddr;

const MAX_LOGIN_RATE_KEY_CHARS: usize = 64;
pub(super) const BACKOFFICE_LOGIN_REALM: &str = "backoffice";
pub(super) const CONSUMER_LOGIN_REALM: &str = "consumer";

/// 将登录域、TCP 来源地址与原始账号组合为两层有界限流 key。
///
/// 合法账号复用领域值对象的 trim 语义；无效输入仍会 trim 并截断，避免
/// 攻击者用超长字符串无限扩大进程内 key。第一层限制单来源总量，第二层限制
/// 来源与账号组合；后台与消费者域分别计数。
pub(super) fn login_rate_keys(realm: &str, peer_ip: IpAddr, account: &str) -> (String, String) {
    let source_key = format!("{realm}|{peer_ip}");
    let account = normalized_rate_account(account);
    let source_account_key = format!("{source_key}|{account}");
    (source_key, source_account_key)
}

/// 按领域规则规范化账号，并为无效输入提供有界的限流标识。
fn normalized_rate_account(account: &str) -> String {
    let account = LoginAccount::new(account)
        .map(LoginAccount::into_string)
        .unwrap_or_else(|_| account.trim().chars().take(MAX_LOGIN_RATE_KEY_CHARS).collect());
    account
}

#[cfg(test)]
mod tests {
    use std::net::{IpAddr, Ipv4Addr};

    use super::{login_rate_keys, BACKOFFICE_LOGIN_REALM, CONSUMER_LOGIN_REALM};

    fn peer(last_octet: u8) -> IpAddr {
        IpAddr::V4(Ipv4Addr::new(192, 0, 2, last_octet))
    }

    #[test]
    fn login_rate_keys_reuse_account_normalization() {
        assert_eq!(
            login_rate_keys(BACKOFFICE_LOGIN_REALM, peer(1), " account01 "),
            (
                "backoffice|192.0.2.1".to_string(),
                "backoffice|192.0.2.1|account01".to_string()
            )
        );
        assert_ne!(
            login_rate_keys(BACKOFFICE_LOGIN_REALM, peer(1), "Account01").1,
            login_rate_keys(BACKOFFICE_LOGIN_REALM, peer(1), "account01").1
        );
    }

    #[test]
    fn login_rate_keys_bound_invalid_input() {
        assert_eq!(
            login_rate_keys(BACKOFFICE_LOGIN_REALM, peer(1), "   ").1,
            "backoffice|192.0.2.1|"
        );
        let key = login_rate_keys(BACKOFFICE_LOGIN_REALM, peer(1), &"x".repeat(100)).1;
        assert_eq!(
            key.strip_prefix("backoffice|192.0.2.1|").unwrap().chars().count(),
            64
        );
    }

    #[test]
    fn login_rate_keys_separate_peer_and_realm() {
        let first = login_rate_keys(BACKOFFICE_LOGIN_REALM, peer(1), "account01");

        assert_ne!(
            first,
            login_rate_keys(BACKOFFICE_LOGIN_REALM, peer(2), "account01")
        );
        assert_ne!(first, login_rate_keys(CONSUMER_LOGIN_REALM, peer(1), "account01"));
    }

    #[test]
    fn source_key_is_shared_by_accounts_from_same_peer_and_realm() {
        let first = login_rate_keys(BACKOFFICE_LOGIN_REALM, peer(1), "account01");
        let second = login_rate_keys(BACKOFFICE_LOGIN_REALM, peer(1), "account02");

        assert_eq!(first.0, second.0);
        assert_ne!(first.1, second.1);
    }
}
