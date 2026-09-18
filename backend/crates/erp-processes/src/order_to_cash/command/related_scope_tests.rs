//! 销售写命令必须在原事务重验所选合同与客户。

/// 越权合同／客户的 create／save／submit 必须在写入事务内 `require_with`。
///
/// # 参数
/// 无。
///
/// # 返回
/// 无。
///
/// # 错误
/// 无。
///
/// # 关键业务约束
/// 禁止再以无范围 `find_by_id` 作为授权读取；handler 事前检查不是唯一凭证。
#[test]
fn create_save_submit_require_related_contract_and_customer_in_write_tx() {
    let auth = include_str!("../authorization.rs");
    let create = include_str!("create.rs");
    let save = include_str!("save.rs");
    let persist = include_str!("../start_approval.rs");
    let reopen = include_str!("../draft_working_copy.rs");
    let handler = include_str!("../../../../../apps/web-api/src/core/handler/sales_order/mod.rs");

    assert!(auth.contains(r#"require_with(self.actor.clone(), "detail", contract_id, executor)"#));
    assert!(auth.contains(r#"require_with(self.actor.clone(), "detail", customer_id, executor)"#));
    assert!(auth.contains("handler 事前检查不能代替"));

    assert!(create.contains("load_contract(contract_id.as_ref(), executor)"));
    assert!(create.contains("load_customer(contract.customer_id.as_ref(), executor)"));
    assert!(
        !create.contains("contracts()\n            .find_by_id(contract_id.as_ref(), &mut NoTransaction)")
    );
    assert!(
        !create.contains("customer_accounts()\n            .find_by_id(&customer_id, &mut NoTransaction)")
    );

    let create_submit_tx = create
        .split("access_for_tx.related_order(&submitted_order, executor)")
        .nth(1)
        .expect("创建并提交事务必须重验合同／客户");
    assert!(create_submit_tx.contains("access_for_tx.creation(&submitted_order, executor)"));
    assert!(create.contains("access_for_tx.related_order(&order_for_tx, executor)"));

    let save_tx = save.split("with_transaction").nth(1).expect("保存写入事务");
    assert!(save_tx.contains("related_order(&order, executor)"));
    assert!(save.contains("resolve_sales_command_draft(&access, &req.contract_id"));

    let persist_tx = persist.split("client").last().expect("提交写入事务");
    assert!(persist_tx.contains("related_order(&order, executor)"));
    assert!(reopen.contains("related_order(&related_order, executor)"));

    assert!(handler.contains("ensure_contract_access"));
    assert!(handler.contains("ensure_customer_access"));
    assert!(handler.contains("sales_command_customer_id(&actor"));
}
