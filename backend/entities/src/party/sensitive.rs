//! 敏感值指纹的带密钥 HMAC-SHA256 实现（数据模型 §4.5.5 / P1 §2.1）。
//!
//! 本模块在实体目录内提供 HMAC-SHA256（hex 编码）与字段规范化函数，
//! 供 `party_bank_account.account_number_query_hmac`、
//! `party_contact.mobile_query_hmac`、`party_address.address_query_hmac`
//! 的指纹生成使用。禁止裸摘要（§4.5.5：低熵敏感值的精确查询必须使用
//! 带密钥的 HMAC，禁止可离线枚举的裸摘要）。
//!
//! 说明：`backend/entities/Cargo.toml` 的 `hmac`/`sha2` 目前位于
//! `[dev-dependencies]`，生产代码不可引用；为保证实体层的 `fingerprint`
//! 公共 API 可被 P3 服务层直接使用，这里以纯标准库实现 SHA-256 与 HMAC
//! 构造，并用 RFC 4231 测试向量锁定正确性。**地基修订候选**：把
//! `hmac`/`sha2` 提升到 `[dependencies]` 后，用标准实现替换本模块内部
//! 算法（走 `chore/erp-p0-amend-*` 流程）。

/// SHA-256 轮常量（FIPS 180-4 §4.2.2）。
const K: [u32; 64] = [
    0x428a_2f98,
    0x7137_4491,
    0xb5c0_fbcf,
    0xe9b5_dba5,
    0x3956_c25b,
    0x59f1_11f1,
    0x923f_82a4,
    0xab1c_5ed5,
    0xd807_aa98,
    0x1283_5b01,
    0x2431_85be,
    0x550c_7dc3,
    0x72be_5d74,
    0x80de_b1fe,
    0x9bdc_06a7,
    0xc19b_f174,
    0xe49b_69c1,
    0xefbe_4786,
    0x0fc1_9dc6,
    0x240c_a1cc,
    0x2de9_2c6f,
    0x4a74_84aa,
    0x5cb0_a9dc,
    0x76f9_88da,
    0x983e_5152,
    0xa831_c66d,
    0xb003_27c8,
    0xbf59_7fc7,
    0xc6e0_0bf3,
    0xd5a7_9147,
    0x06ca_6351,
    0x1429_2967,
    0x27b7_0a85,
    0x2e1b_2138,
    0x4d2c_6dfc,
    0x5338_0d13,
    0x650a_7354,
    0x766a_0abb,
    0x81c2_c92e,
    0x9272_2c85,
    0xa2bf_e8a1,
    0xa81a_664b,
    0xc24b_8b70,
    0xc76c_51a3,
    0xd192_e819,
    0xd699_0624,
    0xf40e_3585,
    0x106a_a070,
    0x19a4_c116,
    0x1e37_6c08,
    0x2748_774c,
    0x34b0_bcb5,
    0x391c_0cb3,
    0x4ed8_aa4a,
    0x5b9c_ca4f,
    0x682e_6ff3,
    0x748f_82ee,
    0x78a5_636f,
    0x84c8_7814,
    0x8cc7_0208,
    0x90be_fffa,
    0xa450_6ceb,
    0xbef9_a3f7,
    0xc671_78f2,
];

/// SHA-256 初始哈希值（FIPS 180-4 §5.3.3）。
const H_INIT: [u32; 8] = [
    0x6a09_e667,
    0xbb67_ae85,
    0x3c6e_f372,
    0xa54f_f53a,
    0x510e_527f,
    0x9b05_688c,
    0x1f83_d9ab,
    0x5be0_cd19,
];

/// 计算 SHA-256 摘要。
///
/// # 参数
/// * `data` - 输入字节
///
/// # 返回
/// 返回 32 字节摘要。
fn sha256(data: &[u8]) -> [u8; 32] {
    let bit_len = (data.len() as u64).wrapping_mul(8);
    let mut padded = Vec::with_capacity(data.len() + 72);
    padded.extend_from_slice(data);
    padded.push(0x80);
    while padded.len() % 64 != 56 {
        padded.push(0);
    }
    padded.extend_from_slice(&bit_len.to_be_bytes());

    let mut hash = H_INIT;
    for chunk in padded.chunks_exact(64) {
        let mut schedule = [0u32; 64];
        for (word, bytes) in schedule.iter_mut().zip(chunk.chunks_exact(4)) {
            *word = u32::from_be_bytes(bytes.try_into().expect("4 字节块"));
        }
        for t in 16..64 {
            let sigma0 = schedule[t - 15].rotate_right(7)
                ^ schedule[t - 15].rotate_right(18)
                ^ (schedule[t - 15] >> 3);
            let sigma1 =
                schedule[t - 2].rotate_right(17) ^ schedule[t - 2].rotate_right(19) ^ (schedule[t - 2] >> 10);
            schedule[t] = schedule[t - 16]
                .wrapping_add(sigma0)
                .wrapping_add(schedule[t - 7])
                .wrapping_add(sigma1);
        }

        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = hash;
        for (round, &constant) in K.iter().enumerate() {
            let sum1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let choose = (e & f) ^ (!e & g);
            let temp1 = h
                .wrapping_add(sum1)
                .wrapping_add(choose)
                .wrapping_add(constant)
                .wrapping_add(schedule[round]);
            let sum0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let majority = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = sum0.wrapping_add(majority);

            h = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }

        let [da, db, dc, dd, de, df, dg, dh] = hash;
        hash = [
            da.wrapping_add(a),
            db.wrapping_add(b),
            dc.wrapping_add(c),
            dd.wrapping_add(d),
            de.wrapping_add(e),
            df.wrapping_add(f),
            dg.wrapping_add(g),
            dh.wrapping_add(h),
        ];
    }

    let mut digest = [0u8; 32];
    for (bytes, word) in digest.chunks_exact_mut(4).zip(hash) {
        bytes.copy_from_slice(&word.to_be_bytes());
    }
    digest
}

/// 计算 HMAC-SHA256 摘要（RFC 2104）。
///
/// # 参数
/// * `key` - 密钥字节
/// * `data` - 消息字节
///
/// # 返回
/// 返回 32 字节摘要。
fn hmac_sha256(key: &[u8], data: &[u8]) -> [u8; 32] {
    let mut key_block = [0u8; 64];
    if key.len() > 64 {
        key_block[..32].copy_from_slice(&sha256(key));
    } else {
        key_block[..key.len()].copy_from_slice(key);
    }

    let ipad: Vec<u8> = key_block.iter().map(|byte| byte ^ 0x36).collect();
    let opad: Vec<u8> = key_block.iter().map(|byte| byte ^ 0x5c).collect();

    let mut inner = ipad;
    inner.extend_from_slice(data);
    let inner_hash = sha256(&inner);

    let mut outer = opad;
    outer.extend_from_slice(&inner_hash);
    sha256(&outer)
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
    const HEX_CHARS: &[u8; 16] = b"0123456789abcdef";
    let mut hex = String::with_capacity(64);
    for byte in hmac_sha256(key, data) {
        hex.push(HEX_CHARS[(byte >> 4) as usize] as char);
        hex.push(HEX_CHARS[(byte & 0x0f) as usize] as char);
    }
    hex
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
    plain
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::{hmac_sha256_hex, normalize_account_number, normalize_address, normalize_mobile};

    /// RFC 4231 测试向量 1–3：锁定 HMAC-SHA256 实现正确性。
    #[test]
    fn matches_rfc4231_test_vectors() {
        let cases: [(&[u8], &[u8], &str); 3] = [
            (
                &[0x0b; 20],
                b"Hi There",
                "b0344c61d8db38535ca8afceaf0bf12b881dc200c9833da726e9376c2e32cff7",
            ),
            (
                b"Jefe",
                b"what do ya want for nothing?",
                "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843",
            ),
            (
                &[0xaa; 20],
                &[0xdd; 50],
                "773ea91e36800e46854db8ebd09181a72959098b3ef8c122d9635514ced565fe",
            ),
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
        assert_eq!(
            normalize_account_number(" 6222-0212_3456 7890 "),
            "6222021234567890"
        );
        assert_eq!(normalize_mobile(" 13800138000 "), "13800138000");
        assert_eq!(
            normalize_address(" 北京市  朝阳区 望京街 10 号 "),
            "北京市 朝阳区 望京街 10 号"
        );
        assert_eq!(normalize_address(""), "");
    }
}
