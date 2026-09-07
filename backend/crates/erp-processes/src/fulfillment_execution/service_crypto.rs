//! 组合层将启动期敏感信息编解码器和附件元数据映射为履约消费方合同。

use erp_fulfillment::entity::facts::{EvidenceRetention, EvidenceSensitivity};
use erp_fulfillment::ports::service_crypto::ServiceLocationCryptoPort;
use erp_party::SensitiveDataCodec;
use erp_support::{RetentionClass, SensitivityClass};

pub(super) struct ServiceCryptoAdapter<'a>(pub(super) &'a SensitiveDataCodec);

impl ServiceLocationCryptoPort for ServiceCryptoAdapter<'_> {
    type Error = crate::Error;

    fn encrypt(&self, plaintext: &str) -> crate::Result<String> {
        Ok(self.0.encrypt(plaintext)?)
    }
}

/// 将附件提供方枚举逐项映射为服务证据的最小事实。
pub(crate) fn evidence_metadata(
    sensitivity: SensitivityClass,
    retention: RetentionClass,
) -> (EvidenceSensitivity, EvidenceRetention) {
    let sensitivity = match sensitivity {
        SensitivityClass::General => EvidenceSensitivity::General,
        SensitivityClass::Sensitive => EvidenceSensitivity::Sensitive,
        SensitivityClass::HighlySensitive => EvidenceSensitivity::HighlySensitive,
    };
    let retention = match retention {
        RetentionClass::LongTerm => EvidenceRetention::LongTerm,
        RetentionClass::ThirtyDays | RetentionClass::SevenDays => EvidenceRetention::Other,
    };
    (sensitivity, retention)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evidence_sensitivity_conversion_preserves_all_provider_categories() {
        for (source, expected) in [
            (SensitivityClass::General, EvidenceSensitivity::General),
            (SensitivityClass::Sensitive, EvidenceSensitivity::Sensitive),
            (
                SensitivityClass::HighlySensitive,
                EvidenceSensitivity::HighlySensitive,
            ),
        ] {
            assert_eq!(
                evidence_metadata(source, RetentionClass::LongTerm),
                (expected, EvidenceRetention::LongTerm)
            );
        }
    }

    #[test]
    fn evidence_retention_conversion_rejects_both_short_retention_categories() {
        for source in [RetentionClass::ThirtyDays, RetentionClass::SevenDays] {
            let (sensitivity, retention) = evidence_metadata(SensitivityClass::Sensitive, source);
            assert_eq!(retention, EvidenceRetention::Other);
            assert!(
                erp_fulfillment::entity::fulfillment::ServiceEvidencePolicy::validate(
                    "image/png",
                    sensitivity,
                    retention,
                    false
                )
                .is_err()
            );
        }
    }
}
