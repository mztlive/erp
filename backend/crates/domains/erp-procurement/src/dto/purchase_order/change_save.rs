//! 采购变更完整目标的独立保存命令。
use application_core::non_blank;
use serde::{Deserialize, Serialize};
use validator::Validate;

use super::{PurchaseChangeDraftView, SavePurchaseOrderLine, SubmitPurchaseChangeRequest};
use crate::Result;
use crate::entity::purchase_order::{
    PurchaseCommandReceipt, PurchaseCommandReceiptIdentity, payload_fingerprint,
};

/// 独立保存采购变更草稿，不创建审批版本。
pub const PURCHASE_CHANGE_SAVE_ACTION: &str = "purchase_change_order.save_draft";

/// 草稿或已驳回采购变更的完整目标保存请求。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct SavePurchaseChangeDraftRequest {
    #[validate(range(min = 1, message = "乐观锁版本必须大于0"))]
    pub expected_lock_version: u64,
    pub payment_term_code: Option<String>,
    #[validate(length(max = 500, message = "变更原因最多500个字符"))]
    pub reason: Option<String>,
    #[validate(length(min = 1, message = "采购变更必须包含目标明细"))]
    pub lines: Vec<SavePurchaseOrderLine>,
    #[validate(custom(function = "non_blank", message = "幂等键不能为空"))]
    #[validate(length(max = 128, message = "幂等键过长"))]
    pub idempotency_key: String,
}

impl SavePurchaseChangeDraftRequest {
    /// 复用提交前的完整目标校验和构造，不执行提交动作。
    /// # 参数
    /// 无。
    /// # 返回
    /// 返回同一目标内容，重提说明为空。
    /// # 错误
    /// 无。
    pub fn target_request(&self) -> SubmitPurchaseChangeRequest {
        SubmitPurchaseChangeRequest {
            expected_lock_version: self.expected_lock_version,
            payment_term_code: self.payment_term_code.clone(),
            reason: self.reason.clone(),
            lines: self.lines.clone(),
            idempotency_key: self.idempotency_key.clone(),
            resubmission_reason: None,
        }
    }

    /// 构造独立保存的稳定命令身份与完整原请求摘要。
    /// # 参数
    /// * `change_id` / `actor_id` - 已授权变更单和操作人。
    /// # 返回
    /// 返回不依赖当前单据状态的身份、请求摘要。
    /// # 错误
    /// 身份无效或请求序列化失败时拒绝。
    pub fn command(
        &self,
        change_id: &str,
        actor_id: &str,
    ) -> Result<(PurchaseCommandReceiptIdentity, String)> {
        let identity = PurchaseCommandReceipt::<PurchaseChangeDraftView>::identity(
            "purchase-change-save-",
            actor_id,
            PURCHASE_CHANGE_SAVE_ACTION,
            Some(change_id),
            self.idempotency_key.trim(),
        )?;
        let fingerprint = payload_fingerprint(
            PURCHASE_CHANGE_SAVE_ACTION,
            change_id,
            &(self.expected_lock_version, &self.payment_term_code, &self.reason, &self.lines),
        )?;
        Ok((identity, fingerprint))
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::entity::purchase_order::PurchaseCommandReceiptError;

    /// 保存命令精确回放首次版本和目标，同键改版本或内容时必须冲突。
    #[test]
    fn save_command_replays_original_target_and_rejects_changed_request() {
        let request: SavePurchaseChangeDraftRequest = serde_json::from_value(json!({
            "expected_lock_version": 3, "payment_term_code": "NET-30", "reason": "成本调整",
            "lines": [{"line_type": "ITEM_SERVICE", "quantity": "2"}], "idempotency_key": "save-1"
        }))
        .unwrap();
        request.validate().unwrap();
        let (identity, fingerprint) = request.command("change", "buyer").unwrap();

        let result = PurchaseChangeDraftView {
            version: 4,
            reason: "成本调整".into(),
            payment_term_code: "NET-30".into(),
            line_keys: vec!["new-line".into()],
            lines: request.lines.clone(),
            base_lines: Vec::new(),
            base_gross_amount: "0".into(),
        };
        let receipt = PurchaseCommandReceipt::new(&identity, &fingerprint, result, "audit".into()).unwrap();
        let restored =
            PurchaseCommandReceipt::decode(receipt.clone(), &identity, &fingerprint).unwrap().into_payload();
        assert_eq!(restored.version, 4);
        assert_eq!(restored.lines[0].quantity.as_deref(), Some("2"));

        let mut changed = request.clone();
        changed.expected_lock_version += 1;
        let (_, altered) = changed.command("change", "buyer").unwrap();
        assert_eq!(
            PurchaseCommandReceipt::decode(receipt.clone(), &identity, &altered).unwrap_err(),
            PurchaseCommandReceiptError::PayloadConflict
        );

        changed = request;
        changed.lines[0].quantity = Some("3".into());
        let (_, altered) = changed.command("change", "buyer").unwrap();
        assert_eq!(
            PurchaseCommandReceipt::decode(receipt, &identity, &altered).unwrap_err(),
            PurchaseCommandReceiptError::PayloadConflict
        );
    }
}
