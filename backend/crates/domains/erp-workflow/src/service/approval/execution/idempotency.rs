//! 审批运行命令的统一幂等身份、当前 V3 协议与历史精确候选。

mod commands;
mod identity;
mod legacy;
mod start;

pub use commands::{
    CancelBlockedIdentityParams, CancelIdentityParams, DocumentCancelIdentityParams, cancel_blocked_identity,
    cancel_identity, decision_identity, document_cancel_identity, resume_identity, upgrade_binding_identity,
};
pub use identity::{
    LegacyReceiptIdentity, PreparedCommandIdentity, ReceiptBranch, command_may_have_committed,
    command_recovery_delay, map_receipt_first_write_error, normalize_idempotency_key, payload_conflict_error,
};
pub use legacy::legacy_payload_digest;
pub use start::{
    StartIdentityParams, legacy_standard_start_receipt_identity, legacy_start_receipt_identity,
    specialized_start_identity, start_identity, start_scope_candidates,
};

#[cfg(test)]
mod tests {
    use bpm::ids::ApprovalCommandReceiptId;
    use bpm::model::{ApprovalCommandReceipt, Timestamp};

    use super::{
        CancelBlockedIdentityParams, CancelIdentityParams, DocumentCancelIdentityParams, ReceiptBranch,
        StartIdentityParams, cancel_blocked_identity, cancel_identity, decision_identity,
        document_cancel_identity, normalize_idempotency_key, resume_identity, start_identity,
        start_scope_candidates, upgrade_binding_identity,
    };

    fn key() -> bpm::model::IdempotencyKey {
        normalize_idempotency_key("  key-1  ").unwrap()
    }

    fn receipt(identity: &super::PreparedCommandIdentity) -> ApprovalCommandReceipt {
        ApprovalCommandReceipt::new(
            ApprovalCommandReceiptId::new("r1"),
            identity.current(),
            "result-1",
            Timestamp::from_unix_secs(1).unwrap(),
        )
        .unwrap()
    }

    fn assert_legacy_replay(identity: &super::PreparedCommandIdentity, scope: &str, digest: String) {
        let mut legacy = receipt(identity);
        legacy.scope_id = scope.to_string();
        legacy.payload_digest = digest;
        assert!(matches!(identity.classify(Some(&legacy)), ReceiptBranch::SamePayload(_)));
    }

    #[test]
    fn execution_idempotency_key_is_canonical_before_lookup() {
        assert_eq!(key().as_str(), "key-1");
        assert!(normalize_idempotency_key("   ").is_err());
        assert!(normalize_idempotency_key(&"k".repeat(129)).is_err());
        assert!(normalize_idempotency_key(&"你".repeat(43)).is_err());
    }

    #[test]
    fn v3_identity_rejects_separator_null_optional_and_unicode_collisions() {
        let separator_left =
            decision_identity(key(), "exec-1", "a\u{1f}b", "APPROVE", Some("c"), 3, "u1").unwrap();
        let separator_right =
            decision_identity(key(), "exec-1", "a", "APPROVE", Some("b\u{1f}c"), 3, "u1").unwrap();
        assert_ne!(separator_left.current().digest(), separator_right.current().digest());

        let none = decision_identity(key(), "exec-1", "wi-1", "APPROVE", None, 3, "用户").unwrap();
        let literal_null =
            decision_identity(key(), "exec-1", "wi-1", "APPROVE", Some("NULL"), 3, "用户").unwrap();
        let empty = decision_identity(key(), "exec-1", "wi-1", "APPROVE", Some(""), 3, "用户").unwrap();
        assert_ne!(none.current().digest(), literal_null.current().digest());
        assert_ne!(none.current().digest(), empty.current().digest());
        assert_ne!(literal_null.current().digest(), empty.current().digest());
    }

    #[test]
    fn current_v3_receipt_cannot_downgrade_to_legacy_digest() {
        let identity = decision_identity(key(), "exec-1", "wi-1", "APPROVE", Some("同意"), 3, "u1").unwrap();
        let mut receipt = receipt(&identity);
        receipt.payload_digest =
            super::legacy::legacy_decision_digest_v2("wi-1", "APPROVE", Some("同意"), 3, "u1");
        assert_eq!(identity.classify(Some(&receipt)), ReceiptBranch::PayloadConflict);
    }

    #[test]
    fn unknown_v3_identity_cannot_be_registered_as_legacy() {
        let rogue_scope = format!("v3:{}", "a".repeat(64));
        let rogue_digest = format!("v3:{}", "b".repeat(64));
        let identity = decision_identity(key(), "exec-1", "wi-1", "APPROVE", None, 3, "u1")
            .unwrap()
            .with_legacy(super::LegacyReceiptIdentity::exact(&rogue_scope, &rogue_digest));
        let mut receipt = receipt(&identity);
        receipt.scope_id = rogue_scope;
        receipt.payload_digest = rogue_digest;
        assert_eq!(identity.classify(Some(&receipt)), ReceiptBranch::PayloadConflict);
    }

    #[test]
    fn legacy_scope_and_digest_are_only_accepted_as_exact_pairs() {
        let identity = decision_identity(key(), "exec-1", "wi-1", "APPROVE", None, 3, "u1").unwrap();
        let mut receipt = receipt(&identity);
        receipt.scope_id = "exec-1".to_string();
        receipt.payload_digest = super::legacy::legacy_decision_digest_v2("wi-1", "APPROVE", None, 3, "u1");
        assert!(matches!(identity.classify(Some(&receipt)), ReceiptBranch::SamePayload(_)));

        receipt.scope_id = identity.current().scope().as_str().to_string();
        assert_eq!(identity.classify(Some(&receipt)), ReceiptBranch::PayloadConflict);
    }

    #[test]
    fn known_legacy_execution_formats_remain_stable() {
        assert_eq!(
            super::legacy::legacy_start_scope("stock_adjustment", "STOCK_ADJUSTMENT", "adj-1", 3),
            "stock_adjustment\u{1f}STOCK_ADJUSTMENT\u{1f}adj-1\u{1f}3"
        );
        assert_eq!(
            super::legacy::legacy_start_digest("def-1", 7, 3, "u1"),
            "bc0d394485a1923f96cf171776c20eaa1c048cfb1849fe4c2972afc59e599202"
        );
        assert_eq!(
            super::legacy::legacy_decision_digest("wi-1", "REJECT", Some("资料不全"), 5, "u1"),
            "17eed8c1056c213f4ba7f4413ee94a96569c0ac7d28d9ddacbeb74466b386fe2"
        );
        assert_eq!(
            super::legacy::legacy_decision_digest_v2("wi-1", "REJECT", Some("资料不全"), 5, "u1"),
            "v2:033257875c9f5c66821f5806a7b0368294ea231a50e6835b1c3dcd3dfb679487"
        );
        assert_eq!(
            super::legacy::legacy_cancel_digest(3, 11, 13, Some(17), "撤回", "u1"),
            "c870d2579b4a9c9a017d8e30577b9c34b24f55a8e408a8ac21a473754c904f4a"
        );
        assert_eq!(
            super::legacy::legacy_document_cancel_digest(3, 7, 11, 13, Some(17), "撤回", "u1"),
            "acb3b94957c7c60ff44e8c54ef57d354ddb09061e8a05332253ebb0d7245954e"
        );
        assert_eq!(
            super::legacy::legacy_resume_digest(11, 13, 17, Some(19), "admin"),
            "bb6ce99d9f5c838095e8809fcf7567ada07a99b3af28287154e6c107385d053b"
        );
        assert_eq!(
            super::legacy::legacy_cancel_blocked_digest("GRAPH_CORRUPTED", 11, 13, None, "人工终止", "admin",),
            "18876a51ff88dc5bcd565918c78ed06240011f3cf5b56acedb745f354fe8da08"
        );
        assert_eq!(
            super::legacy::legacy_cancel_blocked_digest_v2(
                "GRAPH_CORRUPTED",
                11,
                13,
                None,
                "人工终止",
                "admin",
            ),
            "v2:8e3b49b55055600500771e00fd72e19630e49e4c3090e9e5ae967465d57ba9e8"
        );
    }

    #[test]
    fn each_known_legacy_writer_is_read_as_an_exact_pair() {
        let start = start_identity(StartIdentityParams {
            idempotency_key: key(),
            process_kind: "stock_adjustment",
            subject_kind: "STOCK_ADJUSTMENT",
            subject_id: "adj-1",
            subject_version: 3,
            binding_id: "def-1",
            definition_version: 7,
            actor_participant_id: "u1",
        })
        .unwrap();
        assert_legacy_replay(
            &start,
            "stock_adjustment\u{1f}STOCK_ADJUSTMENT\u{1f}adj-1\u{1f}3",
            super::legacy::legacy_start_digest("def-1", 7, 3, "u1"),
        );

        let decision =
            decision_identity(key(), "exec-1", "wi-1", "REJECT", Some("资料不全"), 5, "u1").unwrap();
        assert_legacy_replay(
            &decision,
            "exec-1",
            super::legacy::legacy_decision_digest_v2("wi-1", "REJECT", Some("资料不全"), 5, "u1"),
        );
        assert_legacy_replay(
            &decision,
            "exec-1",
            super::legacy::legacy_decision_digest("wi-1", "REJECT", Some("资料不全"), 5, "u1"),
        );

        let cancel = cancel_identity(CancelIdentityParams {
            idempotency_key: key(),
            instance_id: "inst-1",
            subject_version: 3,
            expected_instance_version: 11,
            expected_execution_version: 13,
            expected_task_version: Some(17),
            reason: "撤回",
            actor_id: "u1",
        })
        .unwrap();
        assert_legacy_replay(
            &cancel,
            "inst-1",
            super::legacy::legacy_cancel_digest(3, 11, 13, Some(17), "撤回", "u1"),
        );

        let document_cancel = document_cancel_identity(DocumentCancelIdentityParams {
            idempotency_key: key(),
            instance_id: "inst-1",
            subject_version: 3,
            expected_document_version: 7,
            expected_instance_version: 11,
            expected_execution_version: 13,
            expected_task_version: Some(17),
            reason: "撤回",
            actor_id: "u1",
        })
        .unwrap();
        assert_legacy_replay(
            &document_cancel,
            "inst-1",
            super::legacy::legacy_document_cancel_digest(3, 7, 11, 13, Some(17), "撤回", "u1"),
        );

        let resume = resume_identity(key(), "inst-1", 11, 13, 17, Some(19), "admin").unwrap();
        assert_legacy_replay(
            &resume,
            "inst-1",
            super::legacy::legacy_resume_digest(11, 13, 17, Some(19), "admin"),
        );

        let blocked = cancel_blocked_identity(CancelBlockedIdentityParams {
            idempotency_key: key(),
            instance_id: "inst-1",
            blocker: "GRAPH_CORRUPTED",
            expected_instance_version: 11,
            expected_execution_version: 13,
            expected_task_version: None,
            reason: "人工终止",
            actor_id: "admin",
        })
        .unwrap();
        assert_legacy_replay(
            &blocked,
            "inst-1",
            super::legacy::legacy_cancel_blocked_digest_v2(
                "GRAPH_CORRUPTED",
                11,
                13,
                None,
                "人工终止",
                "admin",
            ),
        );
        assert_legacy_replay(
            &blocked,
            "inst-1",
            super::legacy::legacy_cancel_blocked_digest("GRAPH_CORRUPTED", 11, 13, None, "人工终止", "admin"),
        );
    }

    #[test]
    fn each_execution_command_has_stable_v3_golden_identity() {
        let start = start_identity(StartIdentityParams {
            idempotency_key: key(),
            process_kind: "stock_adjustment",
            subject_kind: "STOCK_ADJUSTMENT",
            subject_id: "adj-1",
            subject_version: 3,
            binding_id: "def-1",
            definition_version: 7,
            actor_participant_id: "u1",
        })
        .unwrap();
        let decision =
            decision_identity(key(), "exec-1", "wi-1", "REJECT", Some("资料不全"), 5, "u1").unwrap();
        let cancel = cancel_identity(CancelIdentityParams {
            idempotency_key: key(),
            instance_id: "inst-1",
            subject_version: 3,
            expected_instance_version: 11,
            expected_execution_version: 13,
            expected_task_version: Some(17),
            reason: "撤回",
            actor_id: "u1",
        })
        .unwrap();
        let document_cancel = document_cancel_identity(DocumentCancelIdentityParams {
            idempotency_key: key(),
            instance_id: "inst-1",
            subject_version: 3,
            expected_document_version: 7,
            expected_instance_version: 11,
            expected_execution_version: 13,
            expected_task_version: Some(17),
            reason: "撤回",
            actor_id: "u1",
        })
        .unwrap();
        let resume = resume_identity(key(), "inst-1", 11, 13, 17, Some(19), "admin").unwrap();
        let blocked = cancel_blocked_identity(CancelBlockedIdentityParams {
            idempotency_key: key(),
            instance_id: "inst-1",
            blocker: "GRAPH_CORRUPTED",
            expected_instance_version: 11,
            expected_execution_version: 13,
            expected_task_version: None,
            reason: "人工终止",
            actor_id: "admin",
        })
        .unwrap();

        let golden = [
            (
                &start,
                "v3:dc6f7e4d641c5f83f44106a92d943a909406bbec32de038faba6000fba0317b2",
                "v3:cd91ac847e2bc94e7c4076c57db105101e441c3954e91466c514dd7bbe66ccae",
            ),
            (
                &decision,
                "v3:12c197973014a436361dbfdad4160d35c989bde80936e152c95cc1bec261f97a",
                "v3:fefd9fc191a939a7df752237e8deae3d130e96db5f3149282d18891464b4029e",
            ),
            (
                &cancel,
                "v3:b7989e68725984f3097bf2c816f7bb3bf41e6f4e8e7079ad79f9ae875fbfb29e",
                "v3:1c797a695e5b93e6237819b2d9af39240c1e44142450e4d72062db99eafd2a7b",
            ),
            (
                &document_cancel,
                "v3:0370f333bbef0dd4125e2cb1ade4baf7124898455b327f3540a0f6cf7bf9d1eb",
                "v3:6e7abef7185a4a237896dc84e8e11f7996c91d23b0620f6e557072ed7bf1c834",
            ),
            (
                &resume,
                "v3:31f9dd534335a8249ea08e93e6435cb557c92aa877b33a6ada418f6e30cd83a1",
                "v3:68522ea0bb336cd05f318deb7ff2d7167d75e4ce4ae3a68d8ba1f15a6f2fdc10",
            ),
            (
                &blocked,
                "v3:e3586c2ec45553303515c2e70a55aea67227e7190baf495ecf288a1397a84e37",
                "v3:73a859949131f064da4626e37e54705505947d948e67a55ec7411a779a79a066",
            ),
        ];
        for (identity, expected_scope, expected_digest) in golden {
            assert_eq!(identity.current().scope().as_str(), expected_scope);
            assert_eq!(identity.current().digest().as_str(), expected_digest);
        }
    }

    #[test]
    fn start_scope_candidates_pair_current_and_exact_legacy_scope() {
        let scopes = start_scope_candidates("stock_adjustment", "STOCK_ADJUSTMENT", "adj-1", 3).unwrap();
        assert_eq!(scopes.len(), 2);
        assert!(scopes[0].starts_with("v3:"));
        assert_eq!(scopes[1], "stock_adjustment\u{1f}STOCK_ADJUSTMENT\u{1f}adj-1\u{1f}3");
    }

    #[test]
    fn upgrade_binding_is_v3_only_and_collision_free() {
        let exact =
            upgrade_binding_identity("STOCK_ADJUSTMENT", "adj-1", 7, 3, "升级\u{1f}定义", "admin", key())
                .unwrap();
        let relocated = upgrade_binding_identity(
            "STOCK_ADJUSTMENT\u{1f}adj-1",
            "7",
            3,
            0,
            "升级",
            "定义\u{1f}admin",
            key(),
        )
        .unwrap();
        let literal_null =
            upgrade_binding_identity("STOCK_ADJUSTMENT", "adj-1", 7, 3, "NULL", "admin", key()).unwrap();
        let empty = upgrade_binding_identity("STOCK_ADJUSTMENT", "adj-1", 7, 3, "", "admin", key()).unwrap();

        assert_eq!(exact.scope_candidates().len(), 1);
        assert_ne!(exact.current().scope(), relocated.current().scope());
        assert_ne!(exact.current().digest(), relocated.current().digest());
        assert_ne!(literal_null.current().digest(), empty.current().digest());
        assert_eq!(
            exact.current().scope().as_str(),
            "v3:2151197d2b62a05fa8b3f2fe3c6d3e9a5c53db24a43af9efb1f68f584049d444"
        );
        assert_eq!(
            exact.current().digest().as_str(),
            "v3:29020cdbcf32812f24678520e8e94edf0feead816201baace007cd01c5c59731"
        );
    }
}
