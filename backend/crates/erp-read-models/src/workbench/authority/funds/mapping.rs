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
/// 创建人直接来自不可变领域事实，缺失身份不授予创建人参与资格。
pub(in crate::workbench) fn customer_receipt_fact(
    receipt: &CustomerReceipt,
    counterparty: Option<String>,
) -> ObjectFact {
    let mut fact = ObjectFact::new(
        receipt.base.id.clone(),
        format!("回款单 {}", receipt.receipt_no),
        receipt.created_by.clone(),
    );
    fact.counterparty_label = counterparty;
    fact.impact_summary = Some("不审批则回款不能过账、不能核销应收".to_string());
    fact
}
/// 创建人直接来自不可变领域事实，缺失身份不授予创建人参与资格。
pub(in crate::workbench) fn supplier_payment_fact(
    payment: &SupplierPayment,
    counterparty: Option<String>,
) -> ObjectFact {
    let mut fact = ObjectFact::new(
        payment.base.id.clone(),
        format!("供应商付款 {}", payment.payment_no),
        payment.created_by.clone(),
    );
    fact.counterparty_label = counterparty;
    fact.impact_summary = Some("付款已登记并过账；纠错须走付款冲正或供应商退款".to_string());
    fact
}
/// 命令名称优先直接往来，再首选来源，再分录来源；传入的来源 map 必须丢弃缺名称条目。
pub(in crate::workbench) fn customer_refund_fact(
    refund: &CustomerRefund,
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
        refund.created_by.clone(),
    );
    fact.counterparty_label = customer.or_else(|| origin.cloned());
    fact.impact_summary = Some("不审批则客户退款不能过账".to_string());
    fact
}
/// 命令名称优先直接往来，再首选来源，再分录来源；传入的来源 map 必须丢弃缺名称条目。
pub(in crate::workbench) fn supplier_refund_fact(
    refund: &SupplierRefund,
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
        refund.created_by.clone(),
    );
    fact.counterparty_label = supplier.or_else(|| origin.cloned());
    fact.impact_summary = Some("不审批则供应商退款不能过账".to_string());
    fact
}
/// 冲正对象以自身为参与根，名称仅消费命令 counterpart-only 来源。
pub(in crate::workbench) fn receipt_reversal_fact(
    reversal: &ReceiptReversal,
    origins: &HashMap<String, String>,
) -> ObjectFact {
    let mut fact = ObjectFact::new(
        reversal.base.id.clone(),
        format!("回款冲正 {}", reversal.reversal_no),
        reversal.created_by.clone(),
    );
    fact.counterparty_label = origins.get(&reversal.original_customer_receipt_id.to_string()).cloned();
    fact.impact_summary = Some("不审批则回款冲正不能过账".to_string());
    fact
}
/// 冲正对象以自身为参与根，名称仅消费命令 counterpart-only 来源。
pub(in crate::workbench) fn payment_reversal_fact(
    reversal: &PaymentReversal,
    origins: &HashMap<String, String>,
) -> ObjectFact {
    let mut fact = ObjectFact::new(
        reversal.base.id.clone(),
        format!("付款冲正 {}", reversal.reversal_no),
        reversal.created_by.clone(),
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

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use entity_core::BaseModel;
    use erp_workflow::ports::ObjectFact;
    use erp_workflow::service::work_item::access::{ActorAccess, has_object_participation};
    use serde::de::DeserializeOwned;
    use serde_json::json;

    use super::{
        customer_receipt_fact, customer_refund_fact, payment_reversal_fact, receipt_reversal_fact,
        supplier_payment_fact, supplier_refund_fact,
    };
    use crate::workbench::facts::WorkbenchObjectFact;

    /// 构造用于读取投影的六类领域文档；经办人与创建人故意不同。
    fn document<T: DeserializeOwned>(creator: Option<&str>) -> T {
        let mut value = json!({
            "status": "draft",
            "receipt_no": "CR-1", "payment_no": "SP-1", "refund_no": "RF-1", "reversal_no": "RV-1",
            "counterparty_party_id": "party", "customer_id": "customer", "supplier_id": "supplier",
            "amount": "10.00", "received_at": 1, "paid_at": 1, "occurred_at": 1,
            "bank_reference": null, "payee_bank_account_id": "bank", "bank_receipt_asset_id": "asset",
            "original_receipt_id": "receipt", "original_payment_id": "payment",
            "original_customer_receipt_id": "receipt", "original_supplier_payment_id": "payment",
            "original_receivable_entry_id": null, "original_payable_entry_id": null,
            "reason_code": null, "reason_text": "纠错原因", "handled_by": "current-handler",
            "reviewed_by": "current-reviewer", "evidence_attachment_id": null
        });
        let object = value.as_object_mut().unwrap();
        object.extend(serde_json::to_value(BaseModel::fake()).unwrap().as_object().unwrap().clone());
        if let Some(creator) = creator {
            object.insert("created_by".into(), json!(creator));
        }
        serde_json::from_value(value).unwrap()
    }

    /// 执行生产权威映射，与显示共用同一身份来源。
    fn facts(creator: Option<&str>) -> [ObjectFact; 6] {
        let origins = HashMap::new();
        [
            customer_receipt_fact(&document(creator), None),
            supplier_payment_fact(&document(creator), None),
            customer_refund_fact(&document(creator), None, &origins, &origins),
            supplier_refund_fact(&document(creator), None, &origins, &origins),
            receipt_reversal_fact(&document(creator), &origins),
            payment_reversal_fact(&document(creator), &origins),
        ]
    }

    #[test]
    fn six_funds_projections_use_domain_creator_and_keep_handlers_separate() {
        let creator = ActorAccess::new("domain-creator".into());
        let handler = ActorAccess::new("current-handler".into());
        for authority in facts(Some("domain-creator")) {
            assert_eq!(authority.created_by, "domain-creator");
            assert!(has_object_participation(&creator, "", "", &authority));
            assert!(!has_object_participation(&handler, "", "", &authority));
            let brief = WorkbenchObjectFact::from_authority(authority.clone());
            assert_eq!(brief.authority.created_by, authority.created_by);
            assert_eq!(brief.authority.root_document_id, authority.root_document_id);
        }
    }

    #[test]
    fn missing_domain_identity_never_grants_creator_participation() {
        let creator = ActorAccess::new("domain-creator".into());
        let handler = ActorAccess::new("current-handler".into());
        for authority in facts(None) {
            assert!(authority.created_by.is_empty());
            assert!(!has_object_participation(&creator, "", "", &authority));
            assert!(!has_object_participation(&handler, "", "", &authority));
            let brief = WorkbenchObjectFact::from_authority(authority);
            assert!(brief.authority.created_by.is_empty());
        }
    }
}
