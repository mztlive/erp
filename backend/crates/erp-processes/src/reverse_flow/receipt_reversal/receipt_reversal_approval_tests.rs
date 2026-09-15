//! 随原回款冲正命令迁入的八项既有内联合同测试。

use std::str::FromStr;

use erp_core::common::time::Instant;
use erp_core::ids::{CustomerReceiptId, ReceiptReversalId};
use erp_core::money::Amount;
use erp_returns::entity::returns::{ReceiptReversal, ReceiptReversalData, ReceiptReversalStatus};
use erp_returns::service::ReturnsService;
use erp_returns::service::approval::start_receipt_reversal_approval;
use erp_workflow::service::approval::policy::ApprovalDomainAction;

use super::super::adapter::execute_receipt_reversal_domain_action;

fn draft_reversal() -> ReceiptReversal {
    ReceiptReversal::new(
        ReceiptReversalId::new("rr-1"),
        ReceiptReversalData {
            reversal_no: "RR-1".into(),
            original_customer_receipt_id: CustomerReceiptId::new("cr-1"),
            reason_code: None,
            reason_text: "错记回款冲正".into(),
            amount: Amount::from_str("100").expect("金额合法"),
            handled_by: "handler-1".into(),
            reviewed_by: "reviewer-1".into(),
            occurred_at: Instant::from_unix_secs(1),
            evidence_attachment_id: None,
        },
        "creator-1",
    )
    .expect("草稿必须可构造")
}

/// 创建必须注册 BusinessDocument 并绑定发布定义。
#[test]
fn create_registers_document_and_binds_published_definition() {
    let source = concat!(
        include_str!("create.rs"),
        include_str!("commit.rs"),
        include_str!("approval.rs"),
        include_str!("context.rs"),
        include_str!("../receipt_reversal.rs"),
        include_str!("../../../../erp-returns/src/service/receipt_reversal.rs")
    );
    assert!(source.contains("bind_published_definition_on_document_create"));
    assert!(source.contains("new_registered_document"));
    assert!(source.contains("DocumentType::ReceiptReversal"));
    assert!(source.contains("persist_created_receipt_reversal"));
}

/// 本阶段只登记并调用本地对象读取权，不得改写共享闸门。
#[test]
fn create_path_calls_local_object_readable() {
    use super::super::adapter::receipt_reversal_object_readable;

    let production = concat!(
        include_str!("create.rs"),
        include_str!("commit.rs"),
        include_str!("approval.rs"),
        include_str!("context.rs"),
        include_str!("../receipt_reversal.rs"),
        include_str!("../../../../erp-returns/src/service/receipt_reversal.rs")
    );
    assert!(production.contains("receipt_reversal_object_readable"));
    assert!(!production.contains("adapter_object_read_decision"));
    assert!(receipt_reversal_object_readable("org-1", "u1").unwrap());
    assert!(receipt_reversal_object_readable(" ", "u1").is_err());
    assert!(receipt_reversal_object_readable("org-1", "").is_err());
}

/// 提交必须锁定单据、递增 approval_subject_version 并调用 start_approval。
#[test]
fn submit_calls_start_approval_with_subject_version() {
    let source = concat!(
        include_str!("create.rs"),
        include_str!("commit.rs"),
        include_str!("approval.rs"),
        include_str!("context.rs"),
        include_str!("../receipt_reversal.rs"),
        include_str!("../../../../erp-returns/src/service/receipt_reversal.rs")
    );
    assert!(source.contains("pub async fn submit_receipt_reversal"));
    assert!(source.contains("receipt_reversal_start_command"));
    assert!(source.contains("reversal.approval_subject_version"));
    assert!(source.contains("prepare_start"));
}

/// 最终动作唯一为 post_receipt_reversal，且客户端过账旁路关闭。
#[test]
fn final_action_is_post_receipt_reversal() {
    let source = concat!(
        concat!(
            include_str!("create.rs"),
            include_str!("commit.rs"),
            include_str!("approval.rs"),
            include_str!("context.rs"),
            include_str!("../receipt_reversal.rs"),
            include_str!("../../../../erp-returns/src/service/receipt_reversal.rs")
        ),
        include_str!("../receipt_reversal.rs")
    );
    assert!(source.contains("pub async fn post_receipt_reversal"));
    assert!(source.contains("reversal.mark_posted"));
    assert!(source.contains("ReceiptReversalPost"));
    assert!(ReturnsService::reject_receipt_reversal_client_post().is_err());
}

/// 撤回必须调用统一 cancel 并回到草稿。
#[test]
fn cancel_uses_unified_port() {
    let source = concat!(
        include_str!("create.rs"),
        include_str!("commit.rs"),
        include_str!("approval.rs"),
        include_str!("context.rs"),
        include_str!("../receipt_reversal.rs"),
        include_str!("../../../../erp-returns/src/service/receipt_reversal.rs")
    );
    assert!(source.contains("pub async fn cancel_receipt_reversal_approval"));
    assert!(source.contains("prepare_cancel"));
    assert!(source.contains("persist_receipt_reversal_cancel"));
    let _ = ReturnsService::reject_receipt_reversal_client_post();
    let mut reversal = draft_reversal();
    start_receipt_reversal_approval(&mut reversal).unwrap();
    execute_receipt_reversal_domain_action(
        &mut reversal,
        ApprovalDomainAction::ReceiptReversalCancelApproval,
    )
    .unwrap();
    assert_eq!(reversal.status, ReceiptReversalStatus::Draft);
    assert_eq!(reversal.approval_subject_version, 1);
}

/// 生产代码不得保留草稿直接过账或待复核旁路。
#[test]
fn production_closes_draft_post_and_pending_review() {
    let production = concat!(
        include_str!("create.rs"),
        include_str!("commit.rs"),
        include_str!("approval.rs"),
        include_str!("context.rs"),
        include_str!("../receipt_reversal.rs"),
        include_str!("../../../../erp-returns/src/service/receipt_reversal.rs")
    );
    assert!(!production.contains("ReceiptReversalStatus::PendingReview"));
    assert!(!production.contains("Draft =>"));
    assert!(!production.contains("pending_review"));
}

/// 提交/撤回必须使用实体 matches_version，并删除旧 helper。
#[test]
fn version_lock_uses_entity_matches_version() {
    let production = concat!(
        include_str!("create.rs"),
        include_str!("commit.rs"),
        include_str!("approval.rs"),
        include_str!("context.rs"),
        include_str!("../receipt_reversal.rs"),
        include_str!("../../../../erp-returns/src/service/receipt_reversal.rs")
    );
    assert!(production.contains("reversal.matches_version(expected_version)"));
    assert!(production.contains("conflict_if_stale_version"));
    assert!(!production.contains("fn ensure_expected_version"));
}

/// 过账与冲减必须批量读取分录与账户。
#[test]
fn reversal_paths_batch_entry_and_account_reads() {
    let production = include_str!("../../../../erp-finance/src/service/receivable/receipt_reversal.rs")
        .split("#[cfg(test)]")
        .next()
        .expect("生产代码");
    assert!(production.contains("load_receivable_offset_facts"));
    assert!(production.contains("sales_order_ids_for_receipt_allocations"));
    assert!(production.contains("revert_settlement"));
    assert!(!production.contains("find_by_id(&allocation.receivable_entry_id"));
    assert!(!production.contains("find_by_id(&chunk.increase_entry_id"));
    assert!(!production.contains("find_by_id(&entry.receivable_account_id"));
}
