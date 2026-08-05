//! 项目共享的无协调 ID 生成能力。

use uuid::Uuid;

/// 生成新的 UUID v4 字符串 ID。
///
/// 返回不带连字符的 32 位十六进制字符串，不依赖进程内锁或跨实例 worker ID 配置。
pub fn next_id() -> String {
    Uuid::new_v4().simple().to_string()
}

#[cfg(test)]
mod tests {
    use super::next_id;

    #[test]
    fn next_id_returns_distinct_uuid_strings() {
        let first = next_id();
        let second = next_id();

        assert_eq!(first.len(), 32);
        assert!(first.bytes().all(|byte| byte.is_ascii_hexdigit()));
        assert_eq!(second.len(), 32);
        assert!(second.bytes().all(|byte| byte.is_ascii_hexdigit()));
        assert_ne!(first, second);
    }
}
