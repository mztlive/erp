//! Consumer port for keyed content fingerprints used by warehouse revisions.

/// Port warehouse uses to fingerprint sensitive address/contact plaintext.
///
/// The unique HMAC-SHA256 implementation lives in support. Composition adapters
/// must delegate to that function so warehouse does not depend on `erp-support`.
pub trait AttachmentFingerprintPort: Send + Sync {
    /// Return the keyed HMAC-SHA256 hex fingerprint for `plain`.
    ///
    /// # Parameters
    /// * `plain` - plaintext (warehouse currently fingerprints the request value
    ///   before `SensitiveText` trims the encrypted placeholder)
    /// * `key` - HMAC key bytes
    ///
    /// # Returns
    /// 64 lowercase hex characters.
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
