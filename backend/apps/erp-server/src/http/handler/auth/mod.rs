pub mod login;
pub mod profile;

use std::net::IpAddr;

pub(super) const BACKOFFICE_LOGIN_REALM: &str = "backoffice";

/// 将登录域与 TCP 来源地址组合成限流 key。
///
/// # 参数
/// * `realm` - 登录入口，后台为 `backoffice`
/// * `peer_ip` - 连接的 TCP 对端地址
///
/// # 返回
/// 返回 `"{realm}|{peer_ip}"`。同一来源上的不同账号共享这一层配额，账号本身不再单独计数。
///
/// # 错误
/// 无。
pub(super) fn login_source_key(realm: &str, peer_ip: IpAddr) -> String {
    format!("{realm}|{peer_ip}")
}

#[cfg(test)]
mod tests {
    use std::net::{IpAddr, Ipv4Addr};

    use super::{BACKOFFICE_LOGIN_REALM, login_source_key};

    fn peer(last_octet: u8) -> IpAddr {
        IpAddr::V4(Ipv4Addr::new(192, 0, 2, last_octet))
    }

    #[test]
    fn login_source_key_uses_realm_and_peer_only() {
        assert_eq!(login_source_key(BACKOFFICE_LOGIN_REALM, peer(1)), "backoffice|192.0.2.1");
        assert_ne!(
            login_source_key(BACKOFFICE_LOGIN_REALM, peer(1)),
            login_source_key(BACKOFFICE_LOGIN_REALM, peer(2))
        );
    }
}
