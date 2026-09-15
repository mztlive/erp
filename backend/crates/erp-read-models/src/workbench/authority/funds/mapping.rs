//! 资金对象的唯一命令权威映射；显示必须基于同一已读实体调用这些函数。
use std::collections::{HashMap, HashSet};

use erp_finance::entity::payable::{PayableAccount, SupplierPayment};
use erp_finance::entity::receivable::{CustomerReceipt, ReceivableAccount};
use erp_returns::entity::returns::{CustomerRefund, PaymentReversal, ReceiptReversal, SupplierRefund};
use erp_sales::entity::sales_order::SalesOrderRevision;
use erp_workflow::ports::ObjectFact;

use super::super::amount::format_yuan;
/// 仅识别原销售修订上的两个卡券标志，不受富显示卡券行扩展影响。
pub(in crate::workbench) fn voucher_revision_ids(revisions: &[SalesOrderRevision]) -> HashSet<String> {
    revisions
        .iter()
        .filter(|revision| revision.voucher_category_sku_id.is_some() || revision.voucher_expiry_at.is_some())
        .map(|revision| revision.base.id.clone())
        .collect()
}
/// 应收权威影响使用命令的修订标志，创建人来自账户稳定事实。
pub(in crate::workbench) fn receivable_account_fact(
    account: &ReceivableAccount,
    counterparty: Option<String>,
    is_voucher: bool,
) -> ObjectFact {
    let mut fact = ObjectFact::new(
        account.sales_order_id.to_string(),
        format!("应收子账 {}", account.account_seq),
        account.stable.created_by.clone(),
    );
    fact.counterparty_label = counterparty;
    fact.impact_summary = Some(receivable_account_impact(is_voucher));
    fact
}
/// 应付权威标题、参与根和开放金额沿用原已读账户。
pub(in crate::workbench) fn payable_account_fact(
    account: &PayableAccount,
    supplier: Option<String>,
    purchase_no: Option<String>,
) -> ObjectFact {
    let label =
        purchase_no.as_ref().map(|no| format!("采购应付 {no}")).unwrap_or_else(|| "采购应付".to_string());
    let mut fact =
        ObjectFact::new(account.source_document_id.clone(), label, account.stable.created_by.clone());
    fact.counterparty_label = supplier;
    fact.impact_summary = Some(format!("未付金额 {}", format_yuan(&account.open_total)));
    fact
}
/// 使用原创建审计首个 actor 与已读名称，缺 actor 保持空串。
pub(in crate::workbench) fn customer_receipt_fact(
    receipt: &CustomerReceipt,
    created_by: Option<&String>,
    counterparty: Option<String>,
) -> ObjectFact {
    let mut fact = ObjectFact::new(
        receipt.base.id.clone(),
        format!("回款单 {}", receipt.receipt_no),
        created_by.cloned().unwrap_or_default(),
    );
    fact.counterparty_label = counterparty;
    fact.impact_summary = Some("不审批则回款不能过账、不能核销应收".to_string());
    fact
}
/// 使用原创建审计首个 actor 与已读名称，缺 actor 保持空串。
pub(in crate::workbench) fn supplier_payment_fact(
    payment: &SupplierPayment,
    created_by: Option<&String>,
    counterparty: Option<String>,
) -> ObjectFact {
    let mut fact = ObjectFact::new(
        payment.base.id.clone(),
        format!("供应商付款 {}", payment.payment_no),
        created_by.cloned().unwrap_or_default(),
    );
    fact.counterparty_label = counterparty;
    fact.impact_summary = Some("付款已登记并过账；纠错须走付款冲正或供应商退款".to_string());
    fact
}
/// 命令名称优先直接往来，再首选来源，再分录来源；传入的来源 map 必须丢弃缺名称条目。
pub(in crate::workbench) fn customer_refund_fact(
    refund: &CustomerRefund,
    created_by: Option<&String>,
    customer: Option<String>,
    origins: &HashMap<String, String>,
    entry_origins: &HashMap<String, String>,
) -> ObjectFact {
    let origin =
        refund.original_receipt_id.as_ref().and_then(|id| origins.get(&id.to_string())).or_else(|| {
            refund.original_receivable_entry_id.as_ref().and_then(|id| entry_origins.get(&id.to_string()))
        });
    let mut fact = ObjectFact::new(
        refund.base.id.clone(),
        format!("客户退款 {}", refund.refund_no),
        created_by.cloned().unwrap_or_default(),
    );
    fact.counterparty_label = customer.or_else(|| origin.cloned());
    fact.impact_summary = Some("不审批则客户退款不能过账".to_string());
    fact
}
/// 命令名称优先直接往来，再首选来源，再分录来源；传入的来源 map 必须丢弃缺名称条目。
pub(in crate::workbench) fn supplier_refund_fact(
    refund: &SupplierRefund,
    created_by: Option<&String>,
    supplier: Option<String>,
    origins: &HashMap<String, String>,
    entry_origins: &HashMap<String, String>,
) -> ObjectFact {
    let origin =
        refund.original_payment_id.as_ref().and_then(|id| origins.get(&id.to_string())).or_else(|| {
            refund.original_payable_entry_id.as_ref().and_then(|id| entry_origins.get(&id.to_string()))
        });
    let mut fact = ObjectFact::new(
        refund.base.id.clone(),
        format!("供应商退款 {}", refund.refund_no),
        created_by.cloned().unwrap_or_default(),
    );
    fact.counterparty_label = supplier.or_else(|| origin.cloned());
    fact.impact_summary = Some("不审批则供应商退款不能过账".to_string());
    fact
}
/// 冲正对象以自身为参与根，名称仅消费命令 counterpart-only 来源。
pub(in crate::workbench) fn receipt_reversal_fact(
    reversal: &ReceiptReversal,
    created_by: Option<&String>,
    origins: &HashMap<String, String>,
) -> ObjectFact {
    let mut fact = ObjectFact::new(
        reversal.base.id.clone(),
        format!("回款冲正 {}", reversal.reversal_no),
        created_by.cloned().unwrap_or_default(),
    );
    fact.counterparty_label = origins.get(&reversal.original_customer_receipt_id.to_string()).cloned();
    fact.impact_summary = Some("不审批则回款冲正不能过账".to_string());
    fact
}
/// 冲正对象以自身为参与根，名称仅消费命令 counterpart-only 来源。
pub(in crate::workbench) fn payment_reversal_fact(
    reversal: &PaymentReversal,
    created_by: Option<&String>,
    origins: &HashMap<String, String>,
) -> ObjectFact {
    let mut fact = ObjectFact::new(
        reversal.base.id.clone(),
        format!("付款冲正 {}", reversal.reversal_no),
        created_by.cloned().unwrap_or_default(),
    );
    fact.counterparty_label = origins.get(&reversal.original_supplier_payment_id.to_string()).cloned();
    fact.impact_summary = Some("不审批则付款冲正不能过账".to_string());
    fact
}

/// 应收票款影响文案的唯一实现；命令与显示分别传入原有卡券判定结果。
pub(in crate::workbench) fn receivable_account_impact(is_voucher: bool) -> String {
    if is_voucher {
        "不复核则卡券票款、开票与兑付前置事实不能确认".to_string()
    } else {
        "不复核则票款与开票事实不能确认".to_string()
    }
}

/// 开票申请的授权事实来源；申请创建人及销售根对象均取实体。
pub(in crate::workbench) fn invoice_request_fact(
    request: &erp_finance::entity::receivable::SalesInvoiceRequest,
) -> ObjectFact {
    let mut fact = ObjectFact::new(
        request.sales_order_id.to_string(),
        format!("开票申请 {}", request.request_no),
        request.created_by.clone(),
    );
    fact.counterparty_label = Some(request.data.invoice_title.clone());
    fact.impact_summary = Some(format!("申请开票 {} 元，审批通过后交财务开票", request.data.amount));
    fact
}
