//! 仓库修订使用的带密钥内容指纹消费端口。

/// Port warehouse uses to fingerprint sensitive address/contact plaintext.
///
/// The unique HMAC-SHA256 implementation lives in support. Composition adapters
/// must delegate to that function so warehouse does not depend on `erp-support`.
/// 未接线时返回空字符串（调用方 `SensitiveText::new` 以指纹格式错误拒绝；
/// 改为 `Result` 需同步修改组合层 `erp-processes` 适配器，属跨组共享改动，
/// 见 `erp-warehouse-013` 的 `wont_fix` 说明）。
pub trait AttachmentFingerprintPort: Send + Sync {
    /// 返回 `plain` 的带密钥 HMAC-SHA256 十六进制指纹。
    ///
    /// # 参数
    /// * `plain` - 明文（仓库当前在 `SensitiveText` 去首尾空白前对请求原值计算指纹）
    /// * `key` - HMAC 密钥字节
    ///
    /// # 返回
    /// 返回 64 位小写十六进制指纹；未接线时返回空字符串。
    ///
    /// # 错误
    /// 不返回错误。
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
