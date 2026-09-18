//! 敏感值指纹的带密钥 HMAC-SHA256 实现（数据模型 §4.5.5 / P1 §2.1）。
//!
//! 本模块在实体目录内提供 HMAC-SHA256（hex 编码）与字段规范化函数，
//! 供 `party_bank_account.account_number_query_hmac`、
//! `party_contact.mobile_query_hmac`、`party_address.address_query_hmac`
//! 的指纹生成使用。禁止裸摘要（§4.5.5：低熵敏感值的精确查询必须使用
//! 带密钥的 HMAC，禁止可离线枚举的裸摘要）。
//!
//! HMAC-SHA256 使用本 crate 依赖 `hmac` + `sha2`；RFC 4231 测试向量锁定正确性。

use hmac::{Hmac, KeyInit, Mac};
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

/// 计算 HMAC-SHA256 摘要（RFC 2104）。
///
/// HMAC-SHA256 接受任意长度密钥，`new_from_slice` 恒成功。
///
/// # 参数
/// * `key` - 密钥字节
/// * `data` - 消息字节
///
/// # 返回
/// 返回 32 字节摘要。
fn hmac_sha256(key: &[u8], data: &[u8]) -> [u8; 32] {
    let mut mac = HmacSha256::new_from_slice(key).expect("HMAC-SHA256 accepts any key length");
    mac.update(data);
    mac.finalize().into_bytes().into()
}

/// 计算带密钥查询指纹（HMAC-SHA256，hex 编码）。
///
/// # 参数
/// * `key` - 密钥字节（更换密钥后旧指纹全部失效）
/// * `data` - 规范化后的明文
///
/// # 返回
/// 返回 64 位小写 hex 指纹。
pub(crate) fn hmac_sha256_hex(key: &[u8], data: &[u8]) -> String {
    hmac_sha256(key, data).iter().map(|b| format!("{b:02x}")).collect()
}

/// 规范化银行账号：去首尾空白并移除空格、连字符与下划线。
///
/// # 参数
/// * `plain` - 账号明文
///
/// # 返回
/// 返回可用于指纹与精确查询的规范化账号。
pub(crate) fn normalize_account_number(plain: &str) -> String {
    plain
        .chars()
        .filter(|ch| !ch.is_whitespace() && *ch != '-' && *ch != '_')
        .collect::<String>()
        .trim()
        .to_string()
}

/// 规范化手机号：仅去首尾空白（不折叠内部格式）。
///
/// # 参数
/// * `plain` - 手机号明文
///
/// # 返回
/// 返回可用于指纹与精确查询的规范化手机号。
pub(crate) fn normalize_mobile(plain: &str) -> String {
    plain.trim().to_string()
}

/// 规范化地址文本：去首尾空白并折叠内部连续空白为单个空格。
///
/// # 参数
/// * `plain` - 地址明文
///
/// # 返回
/// 返回可用于指纹与精确查询的规范化地址。
pub(crate) fn normalize_address(plain: &str) -> String {
    plain.split_whitespace().collect::<Vec<_>>().join(" ").trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::{hmac_sha256_hex, normalize_account_number, normalize_address, normalize_mobile};

    /// RFC 4231 测试向量 1–3：锁定 HMAC-SHA256 实现正确性。
    #[test]
    fn matches_rfc4231_test_vectors() {
        let cases: [(&[u8], &[u8], &str); 3] = [
            (&[0x0b; 20], b"Hi There", "b0344c61d8db38535ca8afceaf0bf12b881dc200c9833da726e9376c2e32cff7"),
            (
                b"Jefe",
                b"what do ya want for nothing?",
                "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843",
            ),
            (&[0xaa; 20], &[0xdd; 50], "773ea91e36800e46854db8ebd09181a72959098b3ef8c122d9635514ced565fe"),
        ];
        for (key, data, expected) in cases {
            assert_eq!(hmac_sha256_hex(key, data), expected);
        }
    }

    /// 指纹稳定：同密钥同明文结果一致，且与明文可区分。
    #[test]
    fn fingerprint_is_deterministic() {
        let key = b"k1";
        let first = hmac_sha256_hex(key, b"6222021234567890");
        let second = hmac_sha256_hex(key, b"6222021234567890");
        assert_eq!(first, second);
        assert_eq!(first.len(), 64);
        assert_ne!(first, "6222021234567890");
    }

    /// 带密钥：换密钥后指纹不同（禁止无密钥裸摘要）。
    #[test]
    fn fingerprint_changes_with_key() {
        let with_key_a = hmac_sha256_hex(b"key-a", b"6222021234567890");
        let with_key_b = hmac_sha256_hex(b"key-b", b"6222021234567890");
        assert_ne!(with_key_a, with_key_b);
    }

    /// 规范化函数行为：账号移除分隔符、手机号仅去空白、地址折叠空白。
    #[test]
    fn normalizers_behave_as_documented() {
        assert_eq!(normalize_account_number(" 6222-0212_3456 7890 "), "6222021234567890");
        assert_eq!(normalize_mobile(" 13800138000 "), "13800138000");
        assert_eq!(normalize_address(" 北京市  朝阳区 望京街 10 号 "), "北京市 朝阳区 望京街 10 号");
        assert_eq!(normalize_address(""), "");
    }
}
