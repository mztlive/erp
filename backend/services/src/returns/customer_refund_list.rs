//! 客户退款列表视图装配（SALES-R06）。
//!
//! 仓储投影行覆盖 View 所需退款事实；审批绑定由 Service 批量读取后映射。
//! 缺注册行保留退款行，`requirement=PROCESS_REQUIRED` 且 `definition=None`。

use std::collections::HashMap;

use entities::common::time::Instant;
use entities::document_registry::business_document::ApprovalDefinitionBinding;
use entities::document_registry::BusinessDocument;
use entities::money::Amount;
use entities::returns::{CustomerRefund, CustomerRefundStatus};

use super::adapter::document_approval_view;
use super::dto::CustomerRefundView;

/// 列表/详情共用的客户退款事实（不含审批 View）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CustomerRefundListFacts {
    /// 实体主键。
    pub id: String,
    /// 退款单号。
    pub refund_no: String,
    /// 退款状态。
    pub status: CustomerRefundStatus,
    /// 销售退货/拒收处理单。
    pub sales_return_case_id: Option<String>,
    /// 客户。
    pub customer_id: String,
    /// 原回款。
    pub original_receipt_id: Option<String>,
    /// 原应收分录。
    pub original_receivable_entry_id: Option<String>,
    /// 原因代码。
    pub reason_code: Option<String>,
    /// 原因说明。
    pub reason_text: String,
    /// 退款金额。
    pub amount: Amount,
    /// 财务经办人。
    pub handled_by: String,
    /// 财务复核人。
    pub reviewed_by: String,
    /// 实际退款时间。
    pub occurred_at: Instant,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

impl CustomerRefundListFacts {
    /// 从客户退款实体抽取列表/详情共用事实。
    ///
    /// # 参数
    /// * `refund` - 已加载的客户退款实体
    ///
    /// # 返回
    /// 返回与列表投影对齐的退款事实。
    ///
    /// # 错误
    /// 不返回错误。
    ///
    /// # 约束
    /// 不含审批绑定；绑定由调用方批量装载后注入。
    pub fn from_refund(refund: &CustomerRefund) -> Self {
        Self {
            id: refund.base.id.clone(),
            refund_no: refund.refund_no.clone(),
            status: refund.status,
            sales_return_case_id: refund.sales_return_case_id.as_ref().map(ToString::to_string),
            customer_id: refund.customer_id.to_string(),
            original_receipt_id: refund.original_receipt_id.as_ref().map(ToString::to_string),
            original_receivable_entry_id: refund
                .original_receivable_entry_id
                .as_ref()
                .map(ToString::to_string),
            reason_code: refund.reason_code.clone(),
            reason_text: refund.reason_text.clone(),
            amount: refund.amount,
            handled_by: refund.handled_by.clone(),
            reviewed_by: refund.reviewed_by.clone(),
            occurred_at: refund.occurred_at,
            version: refund.base.version,
            created_at: refund.base.created_at,
        }
    }
}

/// 将批量注册行按单据 ID 建索引。
///
/// # 参数
/// * `documents` - `find_documents_by_ids` 返回的乱序注册行
///
/// # 返回
/// 返回 `document_id -> approval_binding` 映射；缺行不出现在结果中。
///
/// # 错误
/// 不返回错误。
///
/// # 约束
/// 不把缺注册行解释为业务失败；调用方对缺键映射只读列表缺省审批摘要。
pub fn index_document_bindings(
    documents: Vec<BusinessDocument>,
) -> HashMap<String, Option<ApprovalDefinitionBinding>> {
    documents
        .into_iter()
        .map(|document| (document.base.id.clone(), document.approval_binding))
        .collect()
}

/// 由本页退款事实与批量注册行装配列表视图。
///
/// # 参数
/// * `facts` - 当前页退款投影事实，顺序即列表顺序
/// * `documents` - `find_documents_by_ids` 返回的乱序注册行
///
/// # 返回
/// 返回与详情字段一致的视图列表；缺注册行保留退款行。
///
/// # 错误
/// 不返回错误。
///
/// # 约束
/// 缺注册行映射 `requirement=PROCESS_REQUIRED`、`definition=None`，
/// 且 `allowed_actions` 为空。
pub fn map_customer_refund_list_page(
    facts: Vec<CustomerRefundListFacts>,
    documents: Vec<BusinessDocument>,
) -> Vec<CustomerRefundView> {
    let bindings = index_document_bindings(documents);
    facts
        .into_iter()
        .map(|item| {
            let binding = bindings.get(&item.id).and_then(Option::as_ref);
            customer_refund_view_from_facts(item, binding)
        })
        .collect()
}

/// 由退款事实与可选绑定装配客户退款视图。
///
/// # 参数
/// * `facts` - 列表投影或详情实体抽取的退款事实
/// * `binding` - 批量读取到的冻结绑定；缺注册行传 `None`
///
/// # 返回
/// 返回与详情字段一致的客户退款视图。
///
/// # 错误
/// 不返回错误。
///
/// # 约束
/// 缺绑定仍保留退款行，`requirement=PROCESS_REQUIRED`、`definition=None`，
/// 且 `allowed_actions` 为空。正式提交/取消/过账不得依据本只读结构授权。
pub fn customer_refund_view_from_facts(
    facts: CustomerRefundListFacts,
    binding: Option<&ApprovalDefinitionBinding>,
) -> CustomerRefundView {
    let mut approval = document_approval_view(binding, None, facts.status);
    if binding.is_none() {
        approval.allowed_actions = Vec::new();
    }
    CustomerRefundView {
        id: facts.id,
        refund_no: facts.refund_no,
        status: facts.status,
        sales_return_case_id: facts.sales_return_case_id,
        customer_id: facts.customer_id,
        original_receipt_id: facts.original_receipt_id,
        original_receivable_entry_id: facts.original_receivable_entry_id,
        reason_code: facts.reason_code,
        reason_text: facts.reason_text,
        amount: facts.amount,
        handled_by: facts.handled_by,
        reviewed_by: facts.reviewed_by,
        occurred_at: facts.occurred_at,
        version: facts.version,
        created_at: facts.created_at,
        approval,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        customer_refund_view_from_facts, index_document_bindings, map_customer_refund_list_page,
        CustomerRefundListFacts,
    };
    use bpm::ApprovalProcessDefinitionId;
    use entities::common::time::Instant;
    use entities::document_registry::business_document::ApprovalDefinitionBinding;
    use entities::document_registry::{BusinessDocument, BusinessDocumentData, DocumentType};
    use entities::ids::{BusinessDocumentId, CustomerAccountId, CustomerReceiptId, CustomerRefundId};
    use entities::money::Amount;
    use entities::returns::{CustomerRefund, CustomerRefundData, CustomerRefundStatus};
    use std::str::FromStr;

    fn facts() -> CustomerRefundListFacts {
        CustomerRefundListFacts {
            id: "crf-1".into(),
            refund_no: "RF-1".into(),
            status: CustomerRefundStatus::Draft,
            sales_return_case_id: Some("src-1".into()),
            customer_id: "cust-1".into(),
            original_receipt_id: Some("cr-1".into()),
            original_receivable_entry_id: None,
            reason_code: Some("QUALITY".into()),
            reason_text: "质量退款".into(),
            amount: Amount::from_str("100.00").expect("金额合法"),
            handled_by: "handler-1".into(),
            reviewed_by: "reviewer-1".into(),
            occurred_at: Instant::from_unix_secs(1_700_000_000),
            version: 1,
            created_at: 1_700_000_000,
        }
    }

    #[test]
    fn missing_registry_row_keeps_refund_and_maps_process_required_without_definition() {
        let view = customer_refund_view_from_facts(facts(), None);
        assert_eq!(view.id, "crf-1");
        assert_eq!(view.refund_no, "RF-1");
        assert_eq!(view.status, CustomerRefundStatus::Draft);
        assert_eq!(view.sales_return_case_id.as_deref(), Some("src-1"));
        assert_eq!(view.customer_id, "cust-1");
        assert_eq!(view.original_receipt_id.as_deref(), Some("cr-1"));
        assert!(view.original_receivable_entry_id.is_none());
        assert_eq!(view.reason_code.as_deref(), Some("QUALITY"));
        assert_eq!(view.reason_text, "质量退款");
        assert_eq!(view.handled_by, "handler-1");
        assert_eq!(view.reviewed_by, "reviewer-1");
        assert_eq!(view.version, 1);
        assert_eq!(view.amount, Amount::from_str("100.00").expect("金额合法"));
        assert_eq!(view.occurred_at, Instant::from_unix_secs(1_700_000_000));
        assert_eq!(view.created_at, 1_700_000_000);
        assert_eq!(view.approval.requirement, "PROCESS_REQUIRED");
        assert!(view.approval.definition.is_none());
        assert!(view.approval.allowed_actions.is_empty(), "空绑定不得授权正式动作");
    }

    #[test]
    fn from_refund_equals_projection_row_mapping_for_same_entity() {
        let refund = CustomerRefund::new(
            CustomerRefundId::new("crf-1"),
            CustomerRefundData {
                refund_no: "RF-1".into(),
                sales_return_case_id: None,
                customer_id: CustomerAccountId::new("cust-1"),
                original_receipt_id: Some(CustomerReceiptId::new("cr-1")),
                original_receivable_entry_id: None,
                reason_code: Some("QUALITY".into()),
                reason_text: "质量退款".into(),
                amount: Amount::from_str("100.50").expect("金额合法"),
                handled_by: "handler-1".into(),
                reviewed_by: "reviewer-1".into(),
                occurred_at: Instant::from_unix_secs(1_700_000_000),
                evidence_attachment_id: None,
            },
            "creator-1",
        )
        .expect("草稿必须可构造");
        let from_row = CustomerRefundListFacts {
            id: refund.base.id.clone(),
            refund_no: refund.refund_no.clone(),
            status: refund.status,
            sales_return_case_id: refund.sales_return_case_id.as_ref().map(ToString::to_string),
            customer_id: refund.customer_id.to_string(),
            original_receipt_id: refund.original_receipt_id.as_ref().map(ToString::to_string),
            original_receivable_entry_id: refund
                .original_receivable_entry_id
                .as_ref()
                .map(ToString::to_string),
            reason_code: refund.reason_code.clone(),
            reason_text: refund.reason_text.clone(),
            amount: refund.amount,
            handled_by: refund.handled_by.clone(),
            reviewed_by: refund.reviewed_by.clone(),
            occurred_at: refund.occurred_at,
            version: refund.base.version,
            created_at: refund.base.created_at,
        };
        assert_eq!(CustomerRefundListFacts::from_refund(&refund), from_row);
        assert_eq!(from_row.amount, refund.amount);
        assert_eq!(from_row.occurred_at, refund.occurred_at);
        assert_eq!(from_row.created_at, refund.base.created_at);
    }

    #[test]
    fn bound_registry_row_exposes_definition_summary() {
        let binding = ApprovalDefinitionBinding::new(
            ApprovalProcessDefinitionId::new("def-1"),
            3,
            Instant::from_unix_secs(1),
        )
        .expect("绑定必须可构造");
        let view = customer_refund_view_from_facts(facts(), Some(&binding));
        let definition = view.approval.definition.expect("绑定必须投影定义");
        assert_eq!(view.approval.requirement, "PROCESS_REQUIRED");
        assert_eq!(definition.id, "def-1");
        assert_eq!(definition.version, 3);
        assert_eq!(view.approval.allowed_actions, vec!["SUBMIT".to_string()]);
    }

    #[test]
    fn index_document_bindings_skips_missing_ids_and_keeps_unbound() {
        let mut bound = BusinessDocument::new(
            BusinessDocumentId::new("crf-bound"),
            BusinessDocumentData {
                document_type: DocumentType::CustomerRefund,
                document_no: "RF-BOUND".into(),
            },
        )
        .expect("注册行必须可构造");
        let unbound = BusinessDocument::new(
            BusinessDocumentId::new("crf-unbound"),
            BusinessDocumentData {
                document_type: DocumentType::CustomerRefund,
                document_no: "RF-UNBOUND".into(),
            },
        )
        .expect("注册行必须可构造");
        let binding = ApprovalDefinitionBinding::new(
            ApprovalProcessDefinitionId::new("def-1"),
            1,
            Instant::from_unix_secs(1),
        )
        .expect("绑定必须可构造");
        bound.bind_approval_definition(binding).expect("绑定必须可挂上");

        let index = index_document_bindings(vec![unbound, bound]);
        assert!(!index.contains_key("crf-missing"));
        assert!(index.get("crf-unbound").expect("未绑定行必须保留").is_none());
        assert!(matches!(index.get("crf-bound"), Some(Some(_))), "绑定行必须保留");
    }

    #[test]
    fn map_page_keeps_missing_registry_rows_in_original_order() {
        let first = facts();
        let mut second = facts();
        second.id = "crf-2".into();
        let views = map_customer_refund_list_page(vec![first, second], Vec::new());
        assert_eq!(views.len(), 2);
        assert_eq!(views[0].id, "crf-1");
        assert_eq!(views[1].id, "crf-2");
        assert!(views.iter().all(|view| {
            view.approval.requirement == "PROCESS_REQUIRED"
                && view.approval.definition.is_none()
                && view.approval.allowed_actions.is_empty()
        }));
    }
}
