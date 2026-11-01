use chrono::Utc;
use config::SupplierTimestampUnit;
use erp_core::common::time::Instant;
use sha1::{Digest, Sha1};
use subtle::ConstantTimeEq;

pub(super) fn sign(channel: &str, timestamp: &str, private_key: &str) -> String {
    let sha = Sha1::digest(format!("channel_no{channel}timestamp{timestamp}{private_key}"));
    // 协议规定 MD5 输入为小写 SHA1 十六进制文本，而非 SHA1 原始字节。
    format!("{:x}", md5::compute(hex::encode(sha)))
}

pub(super) fn signature_matches(expected: &str, supplied: &str) -> bool {
    supplied.len() == 32
        && supplied.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        && bool::from(expected.as_bytes().ct_eq(supplied.as_bytes()))
}

pub(super) fn timestamp(unit: SupplierTimestampUnit) -> String {
    match unit {
        SupplierTimestampUnit::Seconds => Instant::now().unix_secs().to_string(),
        SupplierTimestampUnit::Milliseconds => Utc::now().timestamp_millis().to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn documented_hash_composition_and_lowercase_are_stable() {
        assert_eq!(sign("channel", "1700000000000", "test-key"), "ffa659e628af3c1377bfab18ce2975e2");
        let signature = sign("channel", "1", "key");
        assert!(signature_matches(&signature, &signature));
        assert!(!signature_matches(&signature, &signature.to_uppercase()));
        assert!(!signature_matches(&signature, ""));
    }
}
