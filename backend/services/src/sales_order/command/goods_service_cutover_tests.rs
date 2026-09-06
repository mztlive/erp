/// 实物及服务提交不得再写入采购确认或旧待办。
#[test]
fn goods_service_submit_does_not_create_procurement_confirmation() {
    let source = concat!(
        include_str!("identity.rs"),
        include_str!("create.rs"),
        include_str!("save.rs"),
        include_str!("submit.rs"),
        include_str!("cancel.rs"),
        include_str!("void.rs"),
        include_str!("sellable.rs"),
    );
    let submit = source
        .split("pub async fn submit_sales_order")
        .nth(1)
        .and_then(|body| body.split("pub async fn cancel_approval_submission").next())
        .expect("提交方法");
    assert!(submit.contains("prepare_start"));
    assert!(submit.contains("start_approval_submission"));
    assert!(!submit.contains("ProcurementConfirmation::new"));
    assert!(!submit.contains("WorkItemType::ImportBusinessConfirmation"));
    assert!(!submit.contains("create_procurement_confirmation"));
    assert!(!submit.contains("CARD_SALES_APPROVAL"));
    assert!(!submit.contains("InternalApprovalRuntime"));
    assert!(!submit.contains("FailClosedApprovalActionPort"));
    assert!(!submit.contains("CardSalesManagerApproval"));
    assert!(!submit.contains("CardSalesOperationApproval"));
}

/// 卡券提交必须绑定 VoucherSalesOrder 并走统一启动，不得回退旧路径。
#[test]
fn voucher_create_binds_and_submit_starts_unified_approval() {
    let source = concat!(
        include_str!("identity.rs"),
        include_str!("create.rs"),
        include_str!("save.rs"),
        include_str!("submit.rs"),
        include_str!("cancel.rs"),
        include_str!("void.rs"),
        include_str!("sellable.rs"),
    );
    assert!(source.contains("sales_create_bind_command"));
    assert!(source.contains("crate::sales_order::document_type_of_sales_business"));
    let submit = source
        .split("pub async fn submit_sales_order")
        .nth(1)
        .and_then(|body| body.split("pub async fn cancel_approval_submission").next())
        .expect("提交方法");
    assert!(submit.contains("start_approval_submission"));
    assert!(!submit.contains("CARD_SALES_APPROVAL"));
    assert!(!submit.contains("InternalApprovalRuntime"));
    assert!(!submit.contains("submit_for_review"));
    let create = source
        .split("fn sales_create_bind_command")
        .nth(1)
        .and_then(|body| body.split("async fn persist_bound_sales_document").next())
        .expect("绑定命令");
    assert!(create.contains("crate::sales_order::document_type_of_sales_business"));
    assert!(!create.contains("DocumentType::SalesOrder"));
}

/// 撤回必须构造并调用统一取消端口。
#[test]
fn cancel_calls_unified_prepare_cancel() {
    let source = concat!(
        include_str!("identity.rs"),
        include_str!("create.rs"),
        include_str!("save.rs"),
        include_str!("submit.rs"),
        include_str!("cancel.rs"),
        include_str!("void.rs"),
        include_str!("sellable.rs"),
    );
    let cancel = source
        .split("pub async fn cancel_approval_submission")
        .nth(1)
        .and_then(|body| body.split("async fn replay_sales_submission").next())
        .expect("撤回方法");
    assert!(cancel.contains("prepare_cancel"));
    assert!(cancel.contains("build_sales_order_cancel_input"));
    assert!(cancel.contains("load_cancel_runtime"));
    assert!(cancel.contains("persist_sales_order_cancel"));
}
