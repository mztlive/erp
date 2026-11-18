//! 冲正提交的请求身份和原版本指纹，独立于提交后的单据状态。

use application_core::CommandReceipt;
use erp_core::Result;

use super::{
    SubmitCustomerRefundRequest, SubmitPaymentReversalRequest, SubmitReceiptReversalRequest,
    SubmitSupplierRefundRequest,
};

// 提交退款或冲正时将真实操作人、动作、原单与幂等键绑定到命令身份；载荷取原请求版本，重提说明有值时去除首尾空白后纳入指纹。
fn submit_receipt(
    resource_type: &str,
    id: &str,
    actor_id: &str,
    expected_version: u64,
    idempotency_key: &str,
    resubmission_reason: Option<&str>,
) -> Result<CommandReceipt> {
    let mut fields = vec![id.to_string(), expected_version.to_string()];
    if let Some(reason) = resubmission_reason {
        fields.push("resubmission_reason".into());
        fields.push(reason.trim().to_owned());
    }

    CommandReceipt::from_resource_parts(
        &format!("{resource_type}-submit-"),
        actor_id,
        &format!("{resource_type}.submit"),
        resource_type,
        id,
        idempotency_key,
        fields,
    )
}

impl SubmitReceiptReversalRequest {
    /// 提交回款冲正审批前，将原回款冲正单 ID、实际提交人和幂等键绑定到该动作的命令身份。
    /// 以请求中的原期望版本与规范化后的重提说明形成载荷指纹，不读取原回款冲正单的当前审批轮次；单据或操作号为空时拒绝。
    pub fn command_receipt(&self, id: &str, actor_id: &str) -> Result<CommandReceipt> {
        submit_receipt(
            "receipt_reversal",
            id,
            actor_id,
            self.expected_version,
            &self.idempotency_key,
            self.resubmission_reason.as_deref(),
        )
    }
}

impl SubmitPaymentReversalRequest {
    /// 提交付款冲正审批前，将原付款冲正单 ID、实际提交人和幂等键绑定到该动作的命令身份。
    /// 以请求中的原期望版本与规范化后的重提说明形成载荷指纹，不读取原付款冲正单的当前审批轮次；单据或操作号为空时拒绝。
    pub fn command_receipt(&self, id: &str, actor_id: &str) -> Result<CommandReceipt> {
        submit_receipt(
            "payment_reversal",
            id,
            actor_id,
            self.expected_version,
            &self.idempotency_key,
            self.resubmission_reason.as_deref(),
        )
    }
}

impl SubmitCustomerRefundRequest {
    /// 提交客户退款审批前，将原客户退款单 ID、实际提交人和幂等键绑定到该动作的命令身份。
    /// 以请求中的原期望版本与规范化后的重提说明形成载荷指纹，不读取原客户退款单的当前审批轮次；单据或操作号为空时拒绝。
    pub fn command_receipt(&self, id: &str, actor_id: &str) -> Result<CommandReceipt> {
        submit_receipt(
            "customer_refund",
            id,
            actor_id,
            self.expected_version,
            &self.idempotency_key,
            self.resubmission_reason.as_deref(),
        )
    }
}

impl SubmitSupplierRefundRequest {
    /// 提交供应商退款审批前，将原供应商退款单 ID、实际提交人和幂等键绑定到该动作的命令身份。
    /// 以请求中的原期望版本与规范化后的重提说明形成载荷指纹，不读取原供应商退款单的当前审批轮次；单据或操作号为空时拒绝。
    pub fn command_receipt(&self, id: &str, actor_id: &str) -> Result<CommandReceipt> {
        submit_receipt(
            "supplier_refund",
            id,
            actor_id,
            self.expected_version,
            &self.idempotency_key,
            self.resubmission_reason.as_deref(),
        )
    }
}

#[cfg(test)]
mod tests {
    use application_core::{StructuredCommandReceipt, StructuredReceiptMatch};

    use super::*;

    // 验证两类冲正均以请求原版本匹配回放，同一键的不同版本必须冲突。
    #[test]
    fn reversal_submission_receipts_bind_actor_document_and_original_version() {
        let receipt_request = SubmitReceiptReversalRequest {
            resubmission_reason: None,
            expected_version: 7,
            idempotency_key: "key".into(),
        };
        let payment_request = SubmitPaymentReversalRequest {
            resubmission_reason: None,
            expected_version: 7,
            idempotency_key: "key".into(),
        };

        for (receipt, changed_version) in [
            (
                receipt_request.command_receipt("reversal-1", "handler").unwrap(),
                SubmitReceiptReversalRequest {
                    resubmission_reason: None,
                    expected_version: 8,
                    ..receipt_request.clone()
                }
                .command_receipt("reversal-1", "handler")
                .unwrap(),
            ),
            (
                payment_request.command_receipt("reversal-1", "handler").unwrap(),
                SubmitPaymentReversalRequest {
                    resubmission_reason: None,
                    expected_version: 8,
                    ..payment_request.clone()
                }
                .command_receipt("reversal-1", "handler")
                .unwrap(),
            ),
        ] {
            let fact = StructuredCommandReceipt::from_command(&receipt).unwrap();
            assert_eq!(receipt.match_structured(&fact), StructuredReceiptMatch::SamePayload);
            assert_eq!(changed_version.id(), receipt.id());
            assert_eq!(changed_version.match_structured(&fact), StructuredReceiptMatch::DifferentPayload);

            let mut other_actor = fact.clone();
            other_actor.actor_id = "other".into();
            assert_eq!(receipt.match_structured(&other_actor), StructuredReceiptMatch::Corrupted);
        }

        assert_ne!(
            receipt_request.command_receipt("reversal-1", "handler").unwrap().id(),
            receipt_request.command_receipt("reversal-2", "handler").unwrap().id()
        );
        assert_ne!(
            receipt_request.command_receipt("reversal-1", "handler").unwrap().id(),
            receipt_request.command_receipt("reversal-1", "other").unwrap().id()
        );
        assert_ne!(
            receipt_request.command_receipt("reversal-1", "handler").unwrap().id(),
            payment_request.command_receipt("reversal-1", "handler").unwrap().id()
        );
    }

    // 验证操作号按同一 trim 语义形成身份，空操作号和空资源拒绝进入命令。
    #[test]
    fn reversal_submission_receipts_normalize_keys_and_reject_blank_identity() {
        let req = SubmitReceiptReversalRequest {
            resubmission_reason: None,
            expected_version: 1,
            idempotency_key: " key ".into(),
        };
        let normalized = SubmitReceiptReversalRequest {
            resubmission_reason: None,
            idempotency_key: "key".into(),
            ..req.clone()
        };
        assert_eq!(
            req.command_receipt("rr-1", "actor").unwrap(),
            normalized.command_receipt("rr-1", "actor").unwrap()
        );

        assert!(req.command_receipt(" ", "actor").is_err());
        assert!(
            SubmitPaymentReversalRequest {
                resubmission_reason: None,
                expected_version: 1,
                idempotency_key: " ".into()
            }
            .command_receipt("pr-1", "actor")
            .is_err()
        );
    }

    // 验证重提说明属于幂等载荷；同键改说明冲突，首尾空白不改变语义。
    #[test]
    fn reversal_resubmission_receipts_bind_reason() {
        for resource_type in ["customer_refund", "supplier_refund", "receipt_reversal", "payment_reversal"] {
            let receipt = submit_receipt(resource_type, "id", "actor", 3, "key", Some(" 已核实 ")).unwrap();
            let fact = StructuredCommandReceipt::from_command(&receipt).unwrap();
            let normalized = submit_receipt(resource_type, "id", "actor", 3, "key", Some("已核实")).unwrap();
            assert_eq!(normalized.match_structured(&fact), StructuredReceiptMatch::SamePayload);

            for reason in [None, Some("已修改")] {
                let changed = submit_receipt(resource_type, "id", "actor", 3, "key", reason).unwrap();
                assert_eq!(changed.id(), receipt.id());
                assert_eq!(changed.match_structured(&fact), StructuredReceiptMatch::DifferentPayload);
            }
        }
    }

    // 验证退款请求版本进入独立收据，不随单据后续审批轮次变化。
    #[test]
    fn refund_submission_receipts_bind_original_version() {
        let customer = SubmitCustomerRefundRequest {
            expected_version: 7,
            idempotency_key: "key".into(),
            resubmission_reason: Some("已核实".into()),
        };
        let supplier = SubmitSupplierRefundRequest {
            expected_version: 7,
            idempotency_key: "key".into(),
            resubmission_reason: Some("已核实".into()),
        };

        let pairs = [
            (
                customer.command_receipt("id", "actor").unwrap(),
                SubmitCustomerRefundRequest { expected_version: 8, ..customer }
                    .command_receipt("id", "actor")
                    .unwrap(),
            ),
            (
                supplier.command_receipt("id", "actor").unwrap(),
                SubmitSupplierRefundRequest { expected_version: 8, ..supplier }
                    .command_receipt("id", "actor")
                    .unwrap(),
            ),
        ];

        for (original, changed) in pairs {
            let fact = StructuredCommandReceipt::from_command(&original).unwrap();
            assert_eq!(original.match_structured(&fact), StructuredReceiptMatch::SamePayload);
            assert_eq!(original.id(), changed.id());
            assert_eq!(changed.match_structured(&fact), StructuredReceiptMatch::DifferentPayload);
        }
    }
}
