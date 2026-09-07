//! 真实回执解码的身份、消息和指纹首错合同。
use super::*;

fn identity(payload: &[u8]) -> IntegrationCommandIdentity {
    IntegrationCommandIdentity::new(
        "actor",
        "integration.task_action",
        "work_item",
        "wi-1",
        "same-key",
        payload,
    )
}

fn audit(message: Option<&str>) -> ReceiptAudit<'_> {
    ReceiptAudit {
        actor_id: "actor",
        action: "integration.task_action",
        resource_type: "work_item",
        resource_id: Some("wi-1"),
        message,
    }
}

#[test]
fn receipt_identity_conflicts_precede_missing_or_unparseable_payload() {
    let receipt = identity(b"payload");
    for field in 0..5 {
        let mut stored = audit(Some("invalid-json"));
        let mut current_actor = "actor";
        match field {
            0 => stored.actor_id = "other",
            1 => stored.action = "other",
            2 => stored.resource_type = "other",
            3 => stored.resource_id = None,
            _ => current_actor = "other",
        }
        let result = decode_receipt::<String>(&receipt, current_actor, stored);
        assert!(matches!(result, Err(Error::ConflictError(message)) if message == "幂等键已用于不同命令"));
    }
}

#[test]
fn receipt_missing_message_and_invalid_json_keep_distinct_internal_errors() {
    let receipt = identity(b"payload");
    assert!(
        matches!(decode_receipt::<String>(&receipt,"actor",audit(None)), Err(Error::Internal(message)) if message == "W29 幂等收据缺少结果")
    );
    assert!(
        matches!(decode_receipt::<String>(&receipt,"actor",audit(Some("invalid"))), Err(Error::Internal(message)) if message == "W29 幂等收据不可解析")
    );
}

#[test]
fn same_key_different_payload_is_rejected_and_original_result_replays() {
    let receipt = identity(b"first");
    let changed = identity(b"changed");
    assert_eq!(receipt.receipt_id(), changed.receipt_id());
    let message = serde_json::to_string(&ReceiptEnvelope {
        fingerprint: receipt.fingerprint().to_string(),
        result: "committed",
    })
    .unwrap();
    assert_eq!(
        decode_receipt::<String>(&receipt, "actor", audit(Some(&message))).unwrap(),
        "committed"
    );
    assert!(
        matches!(decode_receipt::<String>(&changed,"actor",audit(Some(&message))), Err(Error::ConflictError(message)) if message == "幂等键已用于不同命令")
    );
}
