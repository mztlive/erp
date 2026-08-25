//! 带密钥 HMAC-SHA256 查询指纹（纯标准库实现，P1 §2.1 / 数据模型 §4.5.5）。
//!
//! 数据模型 §4.5.5 要求低熵敏感值（履约地址等）的精确查询使用带密钥的规范化
//! HMAC，禁止使用可离线枚举的裸摘要。entities 的 `hmac`/`sha2` 目前声明在
//! dev-dependencies（P0 冻结），主库代码不能引用，因此这里用标准库实现
//! SHA-256 与 HMAC 拼装；正确性由测试对照 `hmac`/`sha2` crate 验证
//! （地基修订候选：将 `hmac`/`sha2` 提升为正式依赖并把本实现下沉到 common/，
//! 供 D07/D28 等域复用）。

use crate::errors::{Error, Result};

/// 密钥上限（HMAC 规范块长度；超过则先对密钥做一次 SHA-256）。
const HMAC_BLOCK_LEN: usize = 64;
/// 指纹十六进制长度（SHA-256 摘要 32 字节）。
pub(crate) const FINGERPRINT_HEX_LEN: usize = 64;

/// 计算密钥化 HMAC-SHA256 摘要（十六进制小写）。
///
/// 算法（RFC 2104）：密钥超过块长先哈希；`inner = H(key ^ ipad || data)`，
/// `outer = H(key ^ opad || inner)`。密钥不参与持久化，只用于计算。
///
/// # 参数
/// * `key` - 查询指纹密钥（调用方安全管理，如配置中心下发）
/// * `data` - 敏感值原文字节（规范化后的履约地址等）
///
/// # 返回
/// 返回 64 位小写十六进制指纹。
pub(crate) fn hmac_sha256_hex(key: &[u8], data: &[u8]) -> String {
    let mut key = key.to_vec();
    if key.len() > HMAC_BLOCK_LEN {
        key = sha256(&key).to_vec();
    }
    key.resize(HMAC_BLOCK_LEN, 0);

    let mut ipad = [0x36u8; HMAC_BLOCK_LEN];
    let mut opad = [0x5cu8; HMAC_BLOCK_LEN];
    for (i, byte) in key.iter().enumerate() {
        ipad[i] ^= byte;
        opad[i] ^= byte;
    }

    let mut inner = Vec::with_capacity(HMAC_BLOCK_LEN + data.len());
    inner.extend_from_slice(&ipad);
    inner.extend_from_slice(data);
    let inner_hash = sha256(&inner);

    let mut outer = Vec::with_capacity(HMAC_BLOCK_LEN + 32);
    outer.extend_from_slice(&opad);
    outer.extend_from_slice(&inner_hash);

    sha256(&outer).iter().map(|byte| format!("{byte:02x}")).collect()
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

/// 计算 SHA-256 摘要（FIPS 180-4）。
///
/// # 参数
/// * `data` - 输入字节
///
/// # 返回
/// 返回 32 字节摘要。
fn sha256(data: &[u8]) -> [u8; 32] {
    let mut hash = H0;
    let bit_len = (data.len() as u64).wrapping_mul(8);

    let mut padded = Vec::with_capacity(data.len() + 72);
    padded.extend_from_slice(data);
    padded.push(0x80);
    while padded.len() % 64 != 56 {
        padded.push(0);
    }
    padded.extend_from_slice(&bit_len.to_be_bytes());

    for chunk in padded.chunks_exact(64) {
        let mut w = [0u32; 64];
        for (i, bytes) in chunk.chunks_exact(4).enumerate() {
            w[i] = u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16].wrapping_add(s0).wrapping_add(w[i - 7]).wrapping_add(s1);
        }

        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = hash;
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let temp1 = h
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(maj);
            h = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }

        hash[0] = hash[0].wrapping_add(a);
        hash[1] = hash[1].wrapping_add(b);
        hash[2] = hash[2].wrapping_add(c);
        hash[3] = hash[3].wrapping_add(d);
        hash[4] = hash[4].wrapping_add(e);
        hash[5] = hash[5].wrapping_add(f);
        hash[6] = hash[6].wrapping_add(g);
        hash[7] = hash[7].wrapping_add(h);
    }

    let mut out = [0u8; 32];
    for (i, value) in hash.iter().enumerate() {
        out[i * 4..i * 4 + 4].copy_from_slice(&value.to_be_bytes());
    }
    out
}

/// SHA-256 初始化哈希。
const H0: [u32; 8] = [
    0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
];

/// SHA-256 轮常量（前 64 个素数立方根的小数部分）。
const K: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

#[cfg(test)]
mod tests {
    use super::*;
    use hmac::{Hmac, KeyInit, Mac};
    use sha2::Sha256;

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
        assert_ne!(
            hmac_sha256_hex(b"key-a", plain),
            hmac_sha256_hex(b"key-a", b"address2")
        );
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
}
