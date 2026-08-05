//! 项目共享的无协调 ID 生成能力。
//!
//! 提供两类标识生成能力：
//! - [`next_id`]：UUID v4 内部主键（`id`），主键值不承载业务含义（数据模型 4.1）；
//! - [`DocumentNumberGenerator`]：可展示业务编号（`*_no`）的原子取号，
//!   编号一经形成正式事实不得复用（数据模型 4.1）。

mod document_number;

pub use database::{Executor, NoTransaction};
pub use document_number::{
    format_number, DocumentNumberGenerator, DocumentNumberKind, Error, NumberPhase, Result,
};

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
