use std::collections::BTreeMap;

use entities::supplier_api::{
    BusinessCapabilityConfirmation, BusinessCapabilityConfirmationData, BusinessCapabilityRequirement,
    CapabilityChangeInput, CapabilityChangeSet, CapabilityChangeSetRejection, SupplierApiCapability,
    SupplierApiCapabilityCode, SupplierApiCapabilityData, SupplierApiCapabilityStatus,
    SupplierCommandShapeRejection, SupplierConnectionAction, SupplierHealthCheckType,
};
use erp_core::common::time::Instant;
use erp_core::ids::{SupplierApiCapabilityId, SupplierApiConnectionId};

use crate::errors::Error;
use crate::supplier_api::SupplierConnectionCommand;

use super::command::{apply_validated_changes, map_capability_change_rejection, CommandIdentity};
use super::map_command_shape_rejection;

#[test]
fn command_identity_hashes_raw_idempotency_key() {
    let command = SupplierConnectionCommand {
        action: SupplierConnectionAction::RunHealthCheck,
        expected_version: 1,
        payload_reference: None,
        reason_code: None,
        check_type: Some(SupplierHealthCheckType::Connectivity),
        idempotency_key: "raw-secret-like-key".to_string(),
    };
    let identity = CommandIdentity::new("connection-1", "actor-1", &command).unwrap();
    assert!(!identity.idempotency_hash.contains("raw-secret-like-key"));
    assert_eq!(identity.idempotency_hash.len(), 64);
}

#[test]
fn capability_change_rejections_keep_legacy_error_semantics() {
    assert!(matches!(
        map_capability_change_rejection(CapabilityChangeSetRejection::DuplicateCodes),
        Error::ValidationError(message) if message == "能力变更代码不能重复"
    ));
    assert!(matches!(
        map_capability_change_rejection(
            CapabilityChangeSetRejection::MissingExpectedVersion("order")
        ),
        Error::ValidationError(message) if message == "缺少能力 order 的期望版本"
    ));
    assert!(matches!(
        map_capability_change_rejection(CapabilityChangeSetRejection::UnexpectedExpectedVersion(
            "product".to_string()
        )),
        Error::ValidationError(_)
    ));
    assert!(matches!(
        map_capability_change_rejection(
            CapabilityChangeSetRejection::NewCapabilityVersionMustBeZero("product")
        ),
        Error::ConflictError(message) if message == "新能力 product 的期望版本必须为0"
    ));
    assert!(matches!(
        map_capability_change_rejection(CapabilityChangeSetRejection::NewCapabilityMustStartDisabled(
            "product"
        )),
        Error::BusinessLogicError(_)
    ));
}

#[test]
fn command_shape_rejections_map_to_validation_errors() {
    let error = map_command_shape_rejection(SupplierCommandShapeRejection::TechnicalReferenceOnCreate);
    assert!(matches!(error, Error::ValidationError(_)));
}

/// 构造既有能力声明测试夹具（版本固定为 `1`）。
fn existing_capability(code: SupplierApiCapabilityCode) -> SupplierApiCapability {
    SupplierApiCapability::new(
        SupplierApiCapabilityId::new(format!("cap-{}", code.as_str())),
        SupplierApiCapabilityData {
            connection_id: SupplierApiConnectionId::new("conn-1"),
            capability_code: code,
            status: SupplierApiCapabilityStatus::Disabled,
            constraint_snapshot: None,
        },
    )
    .unwrap()
}

/// 构造覆盖指定能力的采购确认测试夹具。
fn covering_confirmation(capability: &SupplierApiCapability) -> BusinessCapabilityConfirmation {
    BusinessCapabilityConfirmation::new(
        format!("confirm-{}", capability.capability_code.as_str()),
        BusinessCapabilityConfirmationData {
            connection_id: SupplierApiConnectionId::new("conn-1"),
            capability_id: SupplierApiCapabilityId::new(capability.base.id.clone()),
            capability_code: capability.capability_code,
            requirement: BusinessCapabilityRequirement::Required,
            applicability_reference: None,
            evidence_references: vec![],
            reason_code: "REQUIRED".to_string(),
            connection_version: 1,
            capability_version: capability.base.version,
            operation_id: "operation-1".to_string(),
            idempotency_key_hash: "hash-1".to_string(),
            request_fingerprint: "fingerprint-1".to_string(),
            confirmed_by: "buyer-1".to_string(),
            confirmed_at: Instant::from_unix_secs(1),
        },
    )
    .unwrap()
}

#[test]
fn apply_validated_changes_splits_updates_and_creates() {
    let capabilities = vec![existing_capability(SupplierApiCapabilityCode::Order)];
    let confirmations = vec![covering_confirmation(&capabilities[0])];
    let classified = CapabilityChangeSet::new(
        vec![
            CapabilityChangeInput {
                code: SupplierApiCapabilityCode::Order,
                enabled: true,
                constraint_snapshot: None,
            },
            CapabilityChangeInput {
                code: SupplierApiCapabilityCode::Product,
                enabled: false,
                constraint_snapshot: None,
            },
        ],
        &BTreeMap::from([("order".to_string(), 1_u64), ("product".to_string(), 0_u64)]),
    )
    .unwrap()
    .classify(&capabilities)
    .unwrap();

    let (updates, creates) =
        apply_validated_changes("conn-1", &classified, &confirmations, &capabilities).unwrap();
    assert_eq!(updates.len(), 1);
    assert!(updates[0].is_active());
    assert_eq!(creates.len(), 1);
    assert_eq!(creates[0].capability_code, SupplierApiCapabilityCode::Product);
    assert!(!creates[0].is_active());
}

#[test]
fn apply_validated_changes_rejects_version_conflict_and_missing_confirmation() {
    let mut capabilities = vec![existing_capability(SupplierApiCapabilityCode::Order)];
    let classified = CapabilityChangeSet::new(
        vec![CapabilityChangeInput {
            code: SupplierApiCapabilityCode::Order,
            enabled: true,
            constraint_snapshot: None,
        }],
        &BTreeMap::from([("order".to_string(), 1_u64)]),
    )
    .unwrap()
    .classify(&capabilities)
    .unwrap();

    assert!(matches!(
        apply_validated_changes("conn-1", &classified, &[], &capabilities.clone()),
        Err(Error::BusinessLogicError(_))
    ));

    capabilities[0].base.version = 99;
    assert!(matches!(
        apply_validated_changes(
            "conn-1",
            &classified,
            &[covering_confirmation(&existing_capability(
                SupplierApiCapabilityCode::Order
            ))],
            &capabilities
        ),
        Err(Error::ConflictError(_))
    ));
}
