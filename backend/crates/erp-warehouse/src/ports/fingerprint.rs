//! Consumer port for keyed content fingerprints used by warehouse revisions.

/// Port warehouse uses to fingerprint sensitive address/contact plaintext.
///
/// The unique HMAC-SHA256 implementation lives in support. Composition adapters
/// must delegate to that function so warehouse does not depend on `erp-support`.
/// 未接线时返回空字符串（调用方 `SensitiveText::new` 以指纹格式错误拒绝；
/// 改为 `Result` 需同步修改组合层 `erp-processes` 适配器，属跨组共享改动，
/// 见 `erp-warehouse-013` 的 `wont_fix` 说明）。
pub trait AttachmentFingerprintPort: Send + Sync {
    /// Return the keyed HMAC-SHA256 hex fingerprint for `plain`.
    ///
    /// # 参数
    /// * `plain` - 明文（仓库当前在 `SensitiveText` 去首尾空白前对请求原值计算指纹）
    /// * `key` - HMAC 密钥字节
    ///
    /// # 返回
    /// 返回 64 位小写十六进制指纹；未接线时返回空字符串。
    fn content_fingerprint(&self, plain: &str, key: &[u8]) -> String;
}

/// Fail-closed fingerprint port used when composition has not injected an adapter.
#[derive(Debug, Default, Clone, Copy)]
pub struct FailClosedFingerprintPort;

impl AttachmentFingerprintPort for FailClosedFingerprintPort {
    fn content_fingerprint(&self, _plain: &str, _key: &[u8]) -> String {
        String::new()
    }
}
