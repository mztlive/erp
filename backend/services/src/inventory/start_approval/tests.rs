use super::mapping::{
    is_supported_start_receipt_identity, list_projection_from_execution, stock_adjustment_start_digest,
    stock_adjustment_start_identity, stock_adjustment_start_scopes,
};
use super::persist::validate_start_notification_identities;
use crate::approval::execution::idempotency::{legacy_payload_digest, ReceiptBranch};
use bpm::ids::{ApprovalCommandReceiptId, ApprovalNodeExecutionId, ApprovalProcessInstanceId};
use bpm::model::{ApprovalCommandReceipt, ApprovalNodeExecution, NewNodeExecution, ParticipantId, Timestamp};
use entities::approval_integration::ApprovalNotificationEventKind;
use entities::common::time::Instant;
use entities::inventory::{AdjustmentReasonType, MovementDirection};

use crate::inventory::dto::{
    ExpectedStockBalanceVersion, StockAdjustmentLineUpdateInput, SubmitStockAdjustmentRequest,
};

fn assert_start_write_order(source: &str, expected_paths: usize, guard_marker: &str) {
    let production = source.split("#[cfg(test)]").next().expect("生产代码必须存在");
    assert_eq!(
        production.matches(".insert_command_receipt(").count(),
        expected_paths,
        "每条 Fresh 启动路径必须恰有一个 receipt-first 写入"
    );
    assert_eq!(
        production.matches(guard_marker).count(),
        expected_paths,
        "每条 Fresh 启动路径必须恰有一个注册表启动守卫"
    );
    assert_eq!(
        production.matches(".create_bpm_runtime_after_receipt(").count(),
        expected_paths,
        "每条 Fresh 启动路径必须使用 receipt 已仲裁的 BPM 写入口"
    );
    assert!(
        !production.contains(".create_bpm_runtime("),
        "业务启动路径不得回退到 receipt-last 仓储入口"
    );

    let mut cursor = 0;
    for _ in 0..expected_paths {
        let receipt = production[cursor..]
            .find(".insert_command_receipt(")
            .map(|offset| cursor + offset)
            .expect("缺少启动收据写入");
        let guard = production[receipt..]
            .find(guard_marker)
            .map(|offset| receipt + offset)
            .expect("启动收据后缺少注册表守卫");
        let runtime = production[guard..]
            .find(".create_bpm_runtime_after_receipt(")
            .map(|offset| guard + offset)
            .expect("注册表守卫后缺少 BPM 运行事实写入");
        assert!(receipt < guard && guard < runtime);
        cursor = runtime + ".create_bpm_runtime_after_receipt(".len();
    }
}

fn execution() -> ApprovalNodeExecution {
    ApprovalNodeExecution::new_active(NewNodeExecution {
        id: ApprovalNodeExecutionId::new("e1"),
        process_instance_id: ApprovalProcessInstanceId::new("inst-1"),
        node_key: "n1".into(),
        node_name: "仓储复核".into(),
        round_no: 1,
        execution_no: 1,
        assignment_source: bpm::model::types::ApprovalExecutionAssignmentSource::Definition,
        replaces_execution_id: None,
        assignee_participant_id: ParticipantId::new("u1").unwrap(),
        assignee_name_snapshot: "张三".into(),
        at: Timestamp::from_unix_secs(10).unwrap(),
    })
    .expect("入口执行夹具")
}

fn submit_request() -> SubmitStockAdjustmentRequest {
    SubmitStockAdjustmentRequest {
        expected_version: 7,
        expected_subject_version: 2,
        reason_type: AdjustmentReasonType::StockGain,
        lines: vec![
            StockAdjustmentLineUpdateInput {
                line_id: "line-b".to_string(),
                quantity: "2.500".to_string(),
                direction: Some(MovementDirection::Increase),
            },
            StockAdjustmentLineUpdateInput {
                line_id: "line-a".to_string(),
                quantity: "1".to_string(),
                direction: None,
            },
        ],
        balances: vec![
            ExpectedStockBalanceVersion {
                balance_id: "balance-b".to_string(),
                expected_version: 11,
            },
            ExpectedStockBalanceVersion {
                balance_id: "balance-a".to_string(),
                expected_version: 9,
            },
        ],
        note: " 盘点 NULL\u{1f}备注 ".to_string(),
        occurred_at: 42,
        idempotency_key: "submit-1".to_string(),
    }
}

/// 列表投影必须来自入口执行，不得推断未知审批人。
#[test]
fn list_projection_copies_entry_assignee() {
    let projection = list_projection_from_execution(&execution(), Instant::from_unix_secs(10));
    assert_eq!(projection.current_node_key.as_deref(), Some("n1"));
    assert_eq!(projection.current_assignee_participant_id.as_deref(), Some("u1"));
    assert_eq!(projection.current_assignee_name.as_deref(), Some("张三"));
    assert_eq!(projection.last_status_changed_at, Some(10));
}

/// 十一种 PROCESS_REQUIRED 启动类型均固定 receipt -> guard -> BPM 顺序。
#[test]
fn all_process_required_start_paths_are_receipt_first_and_guarded() {
    assert_start_write_order(
        include_str!("../../sales_order/start_approval.rs"),
        1,
        ".mark_approval_started(",
    );
    assert_start_write_order(
        include_str!("../../sales_review/start_approval.rs"),
        1,
        ".mark_approval_started(",
    );
    assert_start_write_order(
        include_str!("../../purchase_order/start_approval.rs"),
        1,
        ".mark_loaded_approval_started(",
    );
    assert_start_write_order(
        include_str!("../../purchase_order/change_start.rs"),
        1,
        ".mark_approval_started(",
    );
    assert_start_write_order(
        include_str!("../../receivable/start_approval.rs"),
        1,
        ".mark_approval_started(",
    );
    assert_start_write_order(
        concat!(
            include_str!("../../returns/start_approval/customer_refund.rs"),
            include_str!("../../returns/start_approval/supplier_refund.rs"),
            include_str!("../../returns/start_approval/receipt_reversal.rs"),
            include_str!("../../returns/start_approval/payment_reversal.rs"),
        ),
        4,
        ".mark_approval_started(",
    );
    assert_start_write_order(include_str!("persist.rs"), 1, ".mark_approval_started(");
}

/// 通用 Start Replay 必须在开启事务前返回，禁止重复写业务事实。
#[test]
fn generic_start_replay_paths_return_before_transaction_writes() {
    for source in [
        include_str!("../../sales_order/start_approval.rs"),
        include_str!("../../sales_review/start_approval.rs"),
        include_str!("../../purchase_order/start_approval.rs"),
        include_str!("../../purchase_order/change_start.rs"),
        include_str!("../../receivable/start_approval.rs"),
        include_str!("../../returns/start_approval/customer_refund.rs"),
        include_str!("../../returns/start_approval/supplier_refund.rs"),
        include_str!("../../returns/start_approval/receipt_reversal.rs"),
        include_str!("../../returns/start_approval/payment_reversal.rs"),
    ] {
        let production = source.split("#[cfg(test)]").next().expect("生产代码必须存在");
        assert!(production.contains("let PreparedExecution::Apply(writes) = prepared else"));
    }
}

/// 库存启动摘要锁定为无歧义 JSON tuple 的字面 SHA-256。
#[test]
fn stock_adjustment_start_digest_has_literal_golden() {
    assert_eq!(
        stock_adjustment_start_digest(&submit_request(), "用户-α").unwrap(),
        "v1:06ca7d6d37aac050a5168f4aa2815e2e40b2c2569530f19ba7e555fd5256683b"
    );
}

/// `NULL`、空值、U+001F 与字段边界不得再产生旧拼接格式碰撞。
#[test]
fn stock_adjustment_start_digest_is_boundary_safe() {
    let normalized = submit_request();
    let mut equivalent = normalized.clone();
    equivalent.lines[1].line_id = " line-a ".to_string();
    equivalent.lines[1].quantity = "1.000".to_string();
    assert_eq!(
        stock_adjustment_start_digest(&normalized, "actor").unwrap(),
        stock_adjustment_start_digest(&equivalent, "actor").unwrap()
    );

    let mut empty = submit_request();
    empty.note.clear();
    let mut literal_null = empty.clone();
    literal_null.note = "NULL".to_string();
    assert_ne!(
        stock_adjustment_start_digest(&empty, "actor").unwrap(),
        stock_adjustment_start_digest(&literal_null, "actor").unwrap()
    );

    let mut left = submit_request();
    left.note = "a\u{1f}42".to_string();
    left.occurred_at = 9;
    let mut right = submit_request();
    right.note = "a".to_string();
    right.occurred_at = 42;
    assert_ne!(
        stock_adjustment_start_digest(&left, "b").unwrap(),
        stock_adjustment_start_digest(&right, "9\u{1f}b").unwrap()
    );
}

/// 每个受签署字段漂移都必须改变 V1 摘要。
#[test]
fn stock_adjustment_start_digest_covers_all_signed_fields() {
    let baseline = submit_request();
    let expected = stock_adjustment_start_digest(&baseline, "actor").unwrap();
    let mut variants = Vec::new();

    let mut value = baseline.clone();
    value.expected_version += 1;
    variants.push(value);
    let mut value = baseline.clone();
    value.expected_subject_version += 1;
    variants.push(value);
    let mut value = baseline.clone();
    value.reason_type = AdjustmentReasonType::Damage;
    variants.push(value);
    let mut value = baseline.clone();
    value.lines[0].quantity = "3".to_string();
    variants.push(value);
    let mut value = baseline.clone();
    value.balances[0].expected_version += 1;
    variants.push(value);
    let mut value = baseline.clone();
    value.note = "不同说明".to_string();
    variants.push(value);
    let mut value = baseline.clone();
    value.occurred_at += 1;
    variants.push(value);

    for value in variants {
        assert_ne!(stock_adjustment_start_digest(&value, "actor").unwrap(), expected);
    }
    assert_ne!(
        stock_adjustment_start_digest(&baseline, "other-actor").unwrap(),
        expected
    );
}

/// V3、库存 V1 与旧 generic 仅允许按各自原 scope/digest 成对回放。
#[test]
fn stock_start_identity_accepts_only_exact_generation_pairs() {
    let req = submit_request();
    let identity = stock_adjustment_start_identity("adj-1", &req, "actor", "def-1", 3).unwrap();
    let current = ApprovalCommandReceipt::new(
        ApprovalCommandReceiptId::new("receipt-1"),
        identity.current(),
        "instance-1",
        Timestamp::from_unix_secs(10).unwrap(),
    )
    .unwrap();
    assert!(matches!(
        identity.classify(Some(&current)),
        ReceiptBranch::SamePayload(_)
    ));

    let scopes = stock_adjustment_start_scopes("adj-1", req.expected_subject_version).unwrap();
    let mut stock_v1 = current.clone();
    stock_v1.scope_id = scopes[1].clone();
    stock_v1.payload_digest = stock_adjustment_start_digest(&req, "actor").unwrap();
    assert!(matches!(
        identity.classify(Some(&stock_v1)),
        ReceiptBranch::SamePayload(_)
    ));

    let mut generic = current.clone();
    generic.scope_id = scopes[1].clone();
    generic.payload_digest = legacy_payload_digest("def-1\u{1f}3\u{1f}2\u{1f}actor");
    assert!(matches!(
        identity.classify(Some(&generic)),
        ReceiptBranch::SamePayload(_)
    ));

    stock_v1.scope_id = scopes[0].clone();
    assert_eq!(identity.classify(Some(&stock_v1)), ReceiptBranch::PayloadConflict);
    generic.payload_digest = stock_adjustment_start_digest(&req, "other-actor").unwrap();
    assert_eq!(identity.classify(Some(&generic)), ReceiptBranch::PayloadConflict);
}

/// Unknown-result 查询按作用域只接受对应世代的摘要形状。
#[test]
fn submit_result_receipt_digest_shape_is_fail_closed() {
    let digest = "06ca7d6d37aac050a5168f4aa2815e2e40b2c2569530f19ba7e555fd5256683b";
    let scopes = vec!["v3-scope".to_string(), "legacy-scope".to_string()];
    assert!(is_supported_start_receipt_identity(
        "v3-scope",
        &format!("v3:{digest}"),
        &scopes,
    ));
    assert!(is_supported_start_receipt_identity(
        "legacy-scope",
        &format!("v1:{digest}"),
        &scopes,
    ));
    assert!(is_supported_start_receipt_identity(
        "legacy-scope",
        digest,
        &scopes,
    ));
    assert!(!is_supported_start_receipt_identity(
        "v3-scope",
        &format!("v1:{digest}"),
        &scopes,
    ));
    assert!(!is_supported_start_receipt_identity(
        "legacy-scope",
        &format!("v3:{digest}"),
        &scopes,
    ));
    assert!(!is_supported_start_receipt_identity(
        "other-scope",
        digest,
        &scopes,
    ));
    assert!(!is_supported_start_receipt_identity(
        "v3-scope",
        &format!("v3:{}", "G".repeat(64)),
        &scopes,
    ));
}

/// 启动计划只允许精确一条 Started 和一条 Entered 通知。
#[test]
fn start_notifications_reject_missing_duplicate_or_extra_intents() {
    let valid = vec![
        (
            ApprovalNotificationEventKind::Started,
            "started:instance-1".to_string(),
        ),
        (
            ApprovalNotificationEventKind::Entered,
            "entered:execution-1".to_string(),
        ),
    ];
    assert!(validate_start_notification_identities(&valid, "instance-1", "execution-1").is_ok());
    assert!(validate_start_notification_identities(&valid[..1], "instance-1", "execution-1").is_err());
    assert!(validate_start_notification_identities(
        &[valid[0].clone(), valid[0].clone()],
        "instance-1",
        "execution-1"
    )
    .is_err());
    let mut extra = valid.clone();
    extra.push((
        ApprovalNotificationEventKind::Completed,
        "completed:instance-1".to_string(),
    ));
    assert!(validate_start_notification_identities(&extra, "instance-1", "execution-1").is_err());
}
