//! 由正式采购批准的加密请求快照合同；解密由消费方负责。

use erp_core::common::time::BusinessDate;
use erp_core::money::Amount;
use serde::{Deserialize, Serialize};

use super::ApiOrderExecution;
use crate::entity::execution_budget::ApprovedExecutionBudget;
use crate::entity::source_purchase::SourcePurchaseTerms;
use crate::ports::connector::offer::DeliveryContext;
use crate::ports::connector::order::{DeliveryChoice, OrderLine, Recipient};
use crate::{Error, Result};

/// 协议请求及配送令牌只在加密边界内序列化，禁止 Debug。
#[derive(Clone, Serialize, Deserialize)]
pub struct ApiOrderRequestSnapshot {
    pub recipient: Recipient,
    pub lines: Vec<OrderLine>,
    pub approved_total: Amount,
    /// 旧快照没有明确分配依据时保持空值，不反推运费已获批准。
    #[serde(default)]
    pub approved_budget: Option<ApprovedExecutionBudget>,
    #[serde(default)]
    pub source_purchase_terms: Vec<FrozenLinePurchaseTerms>,
    pub expected_delivery_date: Option<BusinessDate>,
    pub context: Option<DeliveryContext>,
    pub choices: Vec<DeliveryChoice>,
    pub selected_choice: Option<usize>,
    pub actual_payable: Option<Amount>,
    pub requires_payment: bool,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct FrozenLinePurchaseTerms {
    pub line_id: String,
    pub supplier_offering_revision_id: String,
    pub terms: Option<SourcePurchaseTerms>,
}

/// 从完整请求快照读取精确执行组的批准预算。
/// # 参数
/// `execution` 为当前原执行记录，`decrypted_snapshot` 为其服务端密文的解密内容。
/// # 返回
/// 与执行组、采购修订和快照总额一致的冻结预算。
/// # 错误
/// 完整快照损坏、历史预算缺失或版本身份不一致时保持原错误分类。
pub fn execution_budget(
    execution: &ApiOrderExecution,
    decrypted_snapshot: &str,
) -> Result<ApprovedExecutionBudget> {
    let snapshot: ApiOrderRequestSnapshot = serde_json::from_str(decrypted_snapshot)
        .map_err(|_| Error::Internal("执行组批准预算无法读取".into()))?;

    let budget = snapshot
        .approved_budget
        .ok_or_else(|| Error::BusinessLogicError("原执行未保存可核实的费用分配，请先核查采购依据".into()))?;
    if budget.execution_id != execution.base.id
        || budget.purchase_revision_id != execution.purchase_order_revision_id
        || budget.approved_total != snapshot.approved_total
    {
        return Err(Error::ConflictError("执行组批准预算与原采购版本不一致".into()));
    }
    Ok(budget)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> (ApiOrderExecution, ApiOrderRequestSnapshot) {
        let amount: Amount = "100".parse().unwrap();
        let execution = ApiOrderExecution::queued(
            "execution".into(),
            "purchase".into(),
            "revision".into(),
            "connection".into(),
            "supplier".into(),
            vec![],
        );
        let snapshot = ApiOrderRequestSnapshot {
            recipient: Recipient {
                name: "收件人".into(),
                phone: "13800000000".into(),
                region: "110105".into(),
                address: "某路".into(),
                geocoded_address: None,
                supplier_address: None,
            },
            lines: vec![],
            approved_total: amount,
            approved_budget: Some(ApprovedExecutionBudget {
                purchase_revision_id: "revision".into(),
                execution_id: "execution".into(),
                approved_goods: amount,
                approved_freight: Amount::zero(),
                approved_other: Amount::zero(),
                approved_total: amount,
                freight_allocations: vec![],
            }),
            source_purchase_terms: vec![],
            expected_delivery_date: None,
            context: None,
            choices: vec![],
            selected_choice: None,
            actual_payable: None,
            requires_payment: false,
        };
        (execution, snapshot)
    }

    #[test]
    fn exact_execution_receives_the_frozen_budget_and_mismatched_identity_is_rejected() {
        let (mut execution, snapshot) = fixture();
        let json = serde_json::to_string(&snapshot).unwrap();
        assert_eq!(execution_budget(&execution, &json).unwrap(), snapshot.approved_budget.unwrap());
        execution.purchase_order_revision_id = "another-revision".into();
        assert!(matches!(execution_budget(&execution, &json), Err(Error::ConflictError(_))));
    }

    #[test]
    fn missing_budget_and_incomplete_request_keep_distinct_original_failures() {
        let (execution, mut snapshot) = fixture();
        snapshot.approved_budget = None;
        let mut json = serde_json::to_value(&snapshot).unwrap();
        assert!(matches!(execution_budget(&execution, &json.to_string()), Err(Error::BusinessLogicError(_))));

        json.as_object_mut().unwrap().remove("recipient");
        assert!(matches!(execution_budget(&execution, &json.to_string()), Err(Error::Internal(message))
            if message == "执行组批准预算无法读取"));
    }
}
