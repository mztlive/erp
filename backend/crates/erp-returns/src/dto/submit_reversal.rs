//! 冲正提交的请求身份和原版本指纹，独立于提交后的单据状态。

use application_core::CommandReceipt;
use erp_core::Result;

use super::{SubmitPaymentReversalRequest, SubmitReceiptReversalRequest};

/// 将同一原单、操作人、操作号和请求版本绑定为不可变提交收据。
fn submit_receipt(
    resource_type: &str,
    id: &str,
    actor_id: &str,
    expected_version: u64,
    idempotency_key: &str,
) -> Result<CommandReceipt> {
    CommandReceipt::from_resource_parts(
        &format!("{resource_type}-submit-"),
        actor_id,
        &format!("{resource_type}.submit"),
        resource_type,
        id,
        idempotency_key,
        [id.to_string(), expected_version.to_string()],
    )
}

impl SubmitReceiptReversalRequest {
    /// 构造回款冲正原单提交的稳定命令收据。
    ///
    /// # 参数
    /// * `id` - 原冲正单主键。
    /// * `actor_id` - 当前提交人。
    ///
    /// # 返回
    /// 返回绑定原版本的提交收据。
    ///
    /// # 错误
    /// 单据或操作号为空时返回参数错误。
    pub fn command_receipt(&self, id: &str, actor_id: &str) -> Result<CommandReceipt> {
        submit_receipt("receipt_reversal", id, actor_id, self.expected_version, &self.idempotency_key)
    }
}

impl SubmitPaymentReversalRequest {
    /// 构造付款冲正原单提交的稳定命令收据。
    ///
    /// # 参数
    /// * `id` - 原冲正单主键。
    /// * `actor_id` - 当前提交人。
    ///
    /// # 返回
    /// 返回绑定原版本的提交收据。
    ///
    /// # 错误
    /// 单据或操作号为空时返回参数错误。
    pub fn command_receipt(&self, id: &str, actor_id: &str) -> Result<CommandReceipt> {
        submit_receipt("payment_reversal", id, actor_id, self.expected_version, &self.idempotency_key)
    }
}

#[cfg(test)]
mod tests {
    use application_core::{StructuredCommandReceipt, StructuredReceiptMatch};

    use super::*;

    /// 两类冲正均以请求原版本匹配回放，同一键的不同版本必须冲突。
    #[test]
    fn reversal_submission_receipts_bind_actor_document_and_original_version() {
        let receipt_request =
            SubmitReceiptReversalRequest { expected_version: 7, idempotency_key: "key".into() };
        let payment_request =
            SubmitPaymentReversalRequest { expected_version: 7, idempotency_key: "key".into() };
        for (receipt, changed_version) in [
            (
                receipt_request.command_receipt("reversal-1", "handler").unwrap(),
                SubmitReceiptReversalRequest { expected_version: 8, ..receipt_request.clone() }
                    .command_receipt("reversal-1", "handler")
                    .unwrap(),
            ),
            (
                payment_request.command_receipt("reversal-1", "handler").unwrap(),
                SubmitPaymentReversalRequest { expected_version: 8, ..payment_request.clone() }
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

    /// 操作号按同一 trim 语义形成身份，空操作号和空资源拒绝进入命令。
    #[test]
    fn reversal_submission_receipts_normalize_keys_and_reject_blank_identity() {
        let req = SubmitReceiptReversalRequest { expected_version: 1, idempotency_key: " key ".into() };
        let normalized = SubmitReceiptReversalRequest { idempotency_key: "key".into(), ..req.clone() };
        assert_eq!(
            req.command_receipt("rr-1", "actor").unwrap(),
            normalized.command_receipt("rr-1", "actor").unwrap()
        );
        assert!(req.command_receipt(" ", "actor").is_err());
        assert!(
            SubmitPaymentReversalRequest { expected_version: 1, idempotency_key: " ".into() }
                .command_receipt("pr-1", "actor")
                .is_err()
        );
    }
}
