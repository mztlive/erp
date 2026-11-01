//! 供给命令的 BSON 持久化兼容测试。

use crate::entity::supplier_offering::{SupplierOfferingCommand, SupplierOfferingCommandData};

const ZERO_DIGEST: &str = "0000000000000000000000000000000000000000000000000000000000000000";
const ONE_DIGEST: &str = "1111111111111111111111111111111111111111111111111111111111111111";
const FINGERPRINT_V1_PREFIX: &str = "sha256-v1:";

/// 覆盖指纹版本化与历史裸摘要兼容：前带 `sha256-v1:` 与裸摘要按摘要兼容比较。
#[test]
fn fingerprint_versioned_and_legacy_are_compatible() {
    let bare = command_with(ZERO_DIGEST);
    let versioned = SupplierOfferingCommand::new(
        "cmd-versioned",
        SupplierOfferingCommandData {
            request_fingerprint: format!("{FINGERPRINT_V1_PREFIX}{ZERO_DIGEST}"),
            ..command_data(ZERO_DIGEST)
        },
    )
    .unwrap();
    assert!(bare.ensure_replayable("create_offering", ZERO_DIGEST).is_ok());
    assert!(
        bare.ensure_replayable("create_offering", &format!("{FINGERPRINT_V1_PREFIX}{ZERO_DIGEST}")).is_ok()
    );
    assert!(versioned.ensure_replayable("create_offering", ZERO_DIGEST).is_ok());
    assert!(versioned.ensure_replayable("create_offering", ONE_DIGEST).is_err());
    let doc = bson::serialize_to_document(&bare).unwrap();
    let roundtrip: SupplierOfferingCommand = bson::deserialize_from_document(doc).unwrap();
    assert_eq!(roundtrip.request_fingerprint, ZERO_DIGEST);
}

fn command_with(fingerprint: &str) -> SupplierOfferingCommand {
    SupplierOfferingCommand::new("command-1", command_data(fingerprint)).unwrap()
}

fn command_data(fingerprint: &str) -> SupplierOfferingCommandData {
    SupplierOfferingCommandData {
        idempotency_key: "key-1".to_string(),
        operation: "create_offering".to_string(),
        request_fingerprint: fingerprint.to_string(),
        result_json: "{\"offering_id\":\"o1\"}".to_string(),
    }
}
