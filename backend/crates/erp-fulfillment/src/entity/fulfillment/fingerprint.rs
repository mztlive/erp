//! 带密钥 HMAC-SHA256 查询指纹（P1 §2.1 / 数据模型 §4.5.5）。
//!
//! 数据模型 §4.5.5 要求低熵敏感值（履约地址等）的精确查询使用带密钥的规范化
//! HMAC，禁止使用可离线枚举的裸摘要。HMAC-SHA256 使用本 crate 依赖 `hmac` + `sha2`；
//! RFC 测试向量与对照 crate 实现锁定正确性。

use erp_core::{Error, Result};
use hmac::{Hmac, KeyInit, Mac};
use sha2::Sha256;

/// 指纹十六进制长度（SHA-256 摘要 32 字节）。
pub(crate) const FINGERPRINT_HEX_LEN: usize = 64;

/// 计算密钥化 HMAC-SHA256 摘要（十六进制小写）。
///
/// HMAC-SHA256 接受任意长度密钥，`new_from_slice` 恒成功。
///
/// # 参数
/// * `key` - 查询指纹密钥（调用方安全管理，如配置中心下发）
/// * `data` - 敏感值原文字节（规范化后的履约地址等）
///
/// # 返回
/// 返回 64 位小写十六进制指纹。
pub(crate) fn hmac_sha256_hex(key: &[u8], data: &[u8]) -> String {
    let mut mac = Hmac::<Sha256>::new_from_slice(key).expect("HMAC-SHA256 accepts any key length");
    mac.update(data);
    hex::encode(mac.finalize().into_bytes())
}

/// 校验指纹格式（64 位十六进制，大小写均可）。
///
/// # 参数
/// * `fingerprint` - 待校验的指纹字符串
///
/// # 返回
/// 格式合法返回 `Ok(())`。
///
/// # 错误
/// 长度不是 64 或含非十六进制字符时返回 `LogicError`。
pub(crate) fn validate_fingerprint(fingerprint: &str) -> Result<()> {
    if fingerprint.len() != FINGERPRINT_HEX_LEN || !fingerprint.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(Error::from("查询指纹必须是 64 位十六进制字符串"));
    }
    Ok(())
}

/// 规范化预计算的查询指纹（强类型指纹值对象共用入口）.
///
/// 只处理已计算好的十六进制串；密钥与明文不得传入.
///
/// # 参数
/// * `empty_message` - 空白输入时的错误文案
/// * `too_long_message` - 超长输入时的错误文案
/// * `hex` - 预计算的指纹十六进制串
///
/// # 返回
/// 返回规范化后的指纹串.
///
/// # 错误
/// 空白、超长或非 64 位十六进制时返回错误.
pub(crate) fn normalize_precomputed_fingerprint(
    empty_message: &str,
    too_long_message: &str,
    hex: String,
) -> Result<String> {
    use erp_core::validation::normalize_required_text;

    let value = normalize_required_text(hex, empty_message, FINGERPRINT_HEX_LEN, too_long_message)?;
    validate_fingerprint(&value)?;
    Ok(value)
}

#[cfg(test)]
mod tests {
    use hmac::{Hmac, KeyInit, Mac};
    use sha2::Sha256;

    use super::*;

    type HmacSha256 = Hmac<Sha256>;

    /// 标准测试向量：RFC 4231 与公开 HMAC-SHA256 向量。
    #[test]
    fn matches_known_test_vectors() {
        assert_eq!(
            hmac_sha256_hex(b"key", b"The quick brown fox jumps over the lazy dog"),
            "f7bc83f430538424b13298e6aa6fb143ef4d59a14946175997479dbc2d1a3cd8"
        );
        assert_eq!(
            hmac_sha256_hex(b"", b""),
            "b613679a0814d9ec772f95d778c35fc5ff1697c493715653c6c712144292c5ad"
        );
    }

    /// 与 hmac/sha2 crate 实现逐字节一致（随机密钥与数据）。
    #[test]
    fn matches_reference_crate_implementation() {
        let long_key = b"long-key-".repeat(20);
        let cases: [(&[u8], &[u8]); 4] = [
            (b"secret", "上海市浦东新区xx路1号".as_bytes()),
            (long_key.as_slice(), "收货人张三 13800000000".as_bytes()),
            (b"k", b""),
            (&[], b"address-with-empty-key"),
        ];
        for (key, data) in cases {
            let mut mac = HmacSha256::new_from_slice(key).unwrap();
            mac.update(data);
            let expected = mac.finalize().into_bytes();
            let expected_hex = expected.iter().map(|b| format!("{b:02x}")).collect::<String>();
            assert_eq!(hmac_sha256_hex(key, data), expected_hex);
        }
    }

    /// 指纹带密钥：同一明文不同密钥结果不同；同一密钥结果稳定。
    #[test]
    fn fingerprint_is_keyed_and_deterministic() {
        let plain = b"address";
        assert_eq!(hmac_sha256_hex(b"key-a", plain), hmac_sha256_hex(b"key-a", plain));
        assert_ne!(hmac_sha256_hex(b"key-a", plain), hmac_sha256_hex(b"key-b", plain));
        assert_ne!(hmac_sha256_hex(b"key-a", plain), hmac_sha256_hex(b"key-a", b"address2"));
    }

    /// 校验函数接受小写/大写十六进制，拒绝长度与字符非法值。
    #[test]
    fn fingerprint_format_validation() {
        assert!(validate_fingerprint(&"a".repeat(64)).is_ok());
        assert!(validate_fingerprint(&"A".repeat(64)).is_ok());
        assert!(validate_fingerprint(&"a".repeat(63)).is_err());
        assert!(validate_fingerprint(&"a".repeat(65)).is_err());
        assert!(validate_fingerprint(&format!("{}z", "a".repeat(63))).is_err());
    }

    /// 预计算指纹入口：trim 后校验长度与十六进制；空白/超长/非法字符失败。
    #[test]
    fn precomputed_fingerprint_normalizes_and_validates() {
        use super::normalize_precomputed_fingerprint;

        let padded = format!("  {}  ", "a".repeat(64));
        assert_eq!(normalize_precomputed_fingerprint("不能为空", "过长", padded).unwrap().len(), 64);
        assert!(normalize_precomputed_fingerprint("不能为空", "过长", "  ".to_string()).is_err());
        assert!(normalize_precomputed_fingerprint("不能为空", "过长", "a".repeat(63)).is_err());
        assert!(
            normalize_precomputed_fingerprint("不能为空", "过长", format!("{}z", "a".repeat(63))).is_err()
        );
        let overlong = "a".repeat(65);
        assert_eq!(
            normalize_precomputed_fingerprint("不能为空", "过长哨兵", overlong).unwrap_err().to_string(),
            "过长哨兵"
        );
    }
}
